use std::net::IpAddr;

use super::allowlist::permits;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn an_empty_allowlist_admits_everyone() {
    assert!(permits(&[], ip("203.0.113.9")));
}

#[test]
fn addresses_inside_a_listed_network_are_permitted() {
    let list = vec!["10.0.0.0/8".to_string(), "192.168.1.0/24".to_string()];
    assert!(permits(&list, ip("10.4.5.6")));
    assert!(permits(&list, ip("192.168.1.77")));
}

#[test]
fn addresses_outside_every_network_are_refused() {
    let list = vec!["10.0.0.0/8".to_string(), "192.168.1.0/24".to_string()];
    assert!(!permits(&list, ip("192.168.2.1")));
    assert!(!permits(&list, ip("2001:db8::1")));
}

#[test]
fn ipv4_mapped_ipv6_addresses_match_ipv4_networks() {
    let list = vec!["127.0.0.0/8".to_string()];
    assert!(permits(&list, ip("::ffff:127.0.0.1")));
}

#[test]
fn ipv6_networks_work_too() {
    let list = vec!["fe80::/10".to_string()];
    assert!(permits(&list, ip("fe80::1")));
    assert!(!permits(&list, ip("2001:db8::1")));
}

mod enforcement {
    use mouser_proto::control::{Desk, JoinPending, Method, PendingJoin, ProblemKind, Topic};
    use serde_json::json;

    use crate::testing::{Lab, call, eventually, recv_topic, status};

    #[tokio::test]
    async fn an_address_outside_the_allowlist_is_refused_without_a_prompt() {
        let lab = Lab::new();
        let a = lab
            .start_with("Alpha", mouser_testkit::VirtualPlatform::new(), |c| {
                c.advanced.allowlist = vec!["10.0.0.0/8".into()];
            })
            .await;
        let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
        let b = lab.start("Beta").await;
        let mut a_notes = a.engine.control().notifications();
        let mut b_notes = b.engine.control().notifications();
        let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;

        let refused = recv_topic(&mut a_notes, Topic::Problem).await;
        assert_eq!(refused["kind"], json!(ProblemKind::JoinRefused));
        assert_eq!(refused["computer"], json!(b.engine.computer_id()));
        assert!(
            refused["message"].as_str().unwrap().contains("Beta"),
            "{refused}"
        );
        let told = recv_topic(&mut b_notes, Topic::Problem).await;
        assert!(
            told["message"].as_str().unwrap().contains("not admitted"),
            "{told}"
        );
        eventually("Beta to be unjoined", || async {
            status(&b).await.desk.is_none()
        })
        .await;
        let pending: Vec<PendingJoin> = call(&a, Method::JoinPending, json!({})).await;
        assert!(pending.is_empty(), "no prompt");
    }

    #[tokio::test]
    async fn an_address_inside_the_allowlist_gets_the_prompt() {
        let lab = Lab::new();
        let a = lab
            .start_with("Alpha", mouser_testkit::VirtualPlatform::new(), |c| {
                c.advanced.allowlist = vec!["127.0.0.0/8".into()];
            })
            .await;
        let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
        let b = lab.start("Beta").await;
        let mut a_notes = a.engine.control().notifications();
        let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
        let prompt = recv_topic(&mut a_notes, Topic::JoinRequest).await;
        assert_eq!(prompt["computer"]["name"], "Beta");
    }
}
