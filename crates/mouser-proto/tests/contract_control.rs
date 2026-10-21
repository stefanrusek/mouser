use std::collections::BTreeMap;

use mouser_proto::control::*;
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn rt<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) -> Value {
    let j = serde_json::to_value(v).unwrap();
    assert_eq!(&serde_json::from_value::<T>(j.clone()).unwrap(), v);
    j
}

fn validator() -> jsonschema::Validator {
    let schema: Value =
        serde_json::from_str(include_str!("../schemas/status.schema.json")).unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

fn cid(n: u8) -> ComputerId {
    ComputerId([n; 16])
}

fn problem(kind: ProblemKind, fix: Option<FixAction>) -> Problem {
    Problem {
        problem_id: "missing_permission:accessibility".into(),
        kind,
        computer: cid(2),
        message: "Allow Mouser in Accessibility settings".into(),
        fix,
        raised_at: "2026-05-01T12:00:00Z".into(),
        cleared_at: None,
    }
}

fn status(extra_problem: Problem) -> Status {
    let mut permissions = BTreeMap::new();
    permissions.insert(PermissionKind::Accessibility, PermissionState::Missing);
    permissions.insert(PermissionKind::InputMonitoring, PermissionState::Granted);
    Status {
        version: "0.1.0".into(),
        computer: ComputerRef {
            id: cid(1),
            name: "mac".into(),
        },
        state: EngineState::Active,
        desk: Some(StatusDesk {
            id: DeskId([9; 16]),
            name: "Office".into(),
            pending_join: Some(false),
        }),
        active_computer: Some(cid(1)),
        computers: vec![
            StatusComputer {
                id: cid(1),
                name: "mac".into(),
                os: Os::Macos,
                version: "0.1.0".into(),
                connection: ConnectionState::SelfComputer,
                rtt_ms: None,
                last_seen: None,
                permissions: permissions.clone(),
                limitations: vec!["secure input".into()],
                displays: 2,
            },
            StatusComputer {
                id: cid(2),
                name: "pc".into(),
                os: Os::Windows,
                version: "0.1.0".into(),
                connection: ConnectionState::Connected,
                rtt_ms: Some(1.5),
                last_seen: Some("2026-05-01T12:00:00Z".into()),
                permissions,
                limitations: vec![],
                displays: 1,
            },
        ],
        problems: vec![extra_problem],
        logging: Some(LoggingStatus {
            enabled: true,
            path: "/tmp/log".into(),
        }),
    }
}

#[test]
fn populated_status_validates_against_schema() {
    let v = validator();
    let s = status(problem(
        ProblemKind::MissingPermission,
        Some(FixAction::OpenSystemSettings(PermissionKind::Accessibility)),
    ));
    let j = rt(&s);
    assert!(
        v.is_valid(&j),
        "{:?}",
        v.iter_errors(&j).collect::<Vec<_>>()
    );
    assert_eq!(
        j["problems"][0]["fix"],
        "open_system_settings:accessibility"
    );
    assert_eq!(j["computers"][0]["connection"], "self");
    assert_eq!(j["computers"][0]["permissions"]["accessibility"], "missing");
    assert!(j["computers"][0].get("rtt_ms").is_none());

    // Minimal: no desk, no active computer, no logging, null fix.
    let mut m = status(problem(ProblemKind::PeerOffline, None));
    m.desk = None;
    m.active_computer = None;
    m.logging = None;
    m.state = EngineState::Paused;
    m.computers.clear();
    let j = rt(&m);
    assert!(v.is_valid(&j));
    assert_eq!(j["state"], "paused");
    assert!(j["desk"].is_null() && j.get("logging").is_none());
}

#[test]
fn every_fix_action_validates_and_roundtrips() {
    let v = validator();
    let fixes = [
        FixAction::OpenSystemSettings(PermissionKind::FullDiskAccess),
        FixAction::ApproveJoin,
        FixAction::OpenUpdate,
        FixAction::CrossAnywayHint,
        FixAction::OpenArrangerEdge,
    ];
    for fix in fixes {
        let j = rt(&status(problem(ProblemKind::CrossingBlocked, Some(fix))));
        assert!(v.is_valid(&j));
        assert_eq!(fix.to_string().parse::<FixAction>().unwrap(), fix);
    }
    for bad in ["nope", "open_system_settings:bogus"] {
        let e = bad.parse::<FixAction>().unwrap_err();
        assert_eq!(e.to_string(), format!("unknown fix action: {bad:?}"));
        assert!(serde_json::from_value::<FixAction>(json!(bad)).is_err());
    }
    // postcard path (Problem travels inside the wire Status message)
    let p = problem(ProblemKind::SecureInput, Some(FixAction::OpenUpdate));
    let bytes = postcard::to_stdvec(&p).unwrap();
    assert_eq!(postcard::from_bytes::<Problem>(&bytes).unwrap(), p);
}

#[test]
fn problem_kind_enumerates_exactly_the_contract_set() {
    let kinds = [
        (ProblemKind::MissingPermission, "missing_permission"),
        (ProblemKind::IncompatibleVersion, "incompatible_version"),
        (ProblemKind::JoinRefused, "join_refused"),
        (ProblemKind::SecureInput, "secure_input"),
        (ProblemKind::CrossingBlocked, "crossing_blocked"),
        (ProblemKind::PeerOffline, "peer_offline"),
        (ProblemKind::PlatformLimitation, "platform_limitation"),
    ];
    for (kind, name) in kinds {
        assert_eq!(rt(&kind), json!(name));
    }
    // The schema's enum is the same list.
    let schema: Value =
        serde_json::from_str(include_str!("../schemas/status.schema.json")).unwrap();
    let schema_kinds: Vec<&str> =
        schema["properties"]["problems"]["items"]["properties"]["kind"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
    let ours: Vec<&str> = kinds.iter().map(|(_, n)| *n).collect();
    assert_eq!(schema_kinds, ours);
}

#[test]
fn method_names_match_contract_table() {
    let names = [
        "status",
        "desks.discovered",
        "desk.create",
        "desk.join",
        "desk.leave",
        "desk.move",
        "desk.remove_member",
        "join.pending",
        "join.decide",
        "layout.get",
        "layout.set",
        "config.get",
        "config.set",
        "logging.set",
        "active.pause",
        "active.resume",
        "send_cad",
        "update.check",
        "update.apply",
        "subscribe",
    ];
    assert_eq!(Method::ALL.map(Method::name), names);
    for m in Method::ALL {
        assert_eq!(Method::from_name(m.name()), Some(m));
    }
    assert_eq!(Method::from_name("nope"), None);
}

#[test]
fn envelope_roundtrips() {
    let req = Request::new(7, Method::DeskCreate, Some(json!({"name": "Office"})));
    assert_eq!(
        rt(&req),
        json!({"jsonrpc":"2.0","id":7,"method":"desk.create","params":{"name":"Office"}})
    );
    let no_params = rt(&Request::new(8, Method::Status, None));
    assert!(no_params.get("params").is_none());

    let ok = Response::ok(7, json!({}));
    assert_eq!(rt(&ok), json!({"jsonrpc":"2.0","id":7,"result":{}}));
    let err = Response::err(
        9,
        RpcError {
            code: -32000,
            message: "boom".into(),
            data: Some(ErrorData {
                kind: ProblemKind::PeerOffline,
            }),
        },
    );
    assert_eq!(
        rt(&err),
        json!({"jsonrpc":"2.0","id":9,"error":{"code":-32000,"message":"boom","data":{"kind":"peer_offline"}}})
    );
    let bare = RpcError {
        code: -32601,
        message: "no".into(),
        data: None,
    };
    assert!(rt(&bare).get("data").is_none());

    let n = Notification {
        jsonrpc: "2.0".into(),
        method: Topic::JoinRequest,
        params: json!({"request_id": 1}),
    };
    assert_eq!(rt(&n)["method"], "join_request");
    for (t, name) in [
        (Topic::Problem, "problem"),
        (Topic::Transfer, "transfer"),
        (Topic::Status, "status"),
    ] {
        assert_eq!(rt(&t), json!(name));
    }
}

#[test]
fn every_method_params_and_result_serialize() {
    let desk = DeskId([4; 16]);
    let hex_desk = "04".repeat(16);

    // status result is covered above. desks.discovered
    let d = DiscoveredDesk {
        desk_id: desk,
        name: "Office".into(),
        members: vec!["mac".into()],
    };
    assert_eq!(
        rt(&d),
        json!({"desk_id": hex_desk, "name":"Office","members":["mac"]})
    );
    // desk.create
    assert_eq!(
        rt(&DeskCreateParams { name: "A".into() }),
        json!({"name":"A"})
    );
    let state = DeskState {
        epoch: 1,
        author: ComputerId([5; 16]),
        members: vec![],
        layout: Layout::default(),
        allowlist: vec![],
    };
    let created = Desk {
        desk_id: desk,
        name: "A".into(),
        state,
    };
    assert_eq!(rt(&created)["state"]["epoch"], 1);
    // desk.join both forms, desk.move, pending result
    assert_eq!(
        rt(&DeskJoinParams::ById { desk_id: desk }),
        json!({"desk_id": hex_desk})
    );
    assert_eq!(
        rt(&DeskJoinParams::ByAddress {
            address: "10.0.0.2:47801".into()
        }),
        json!({"address":"10.0.0.2:47801"})
    );
    assert_eq!(
        rt(&DeskMoveParams { desk_id: desk }),
        json!({"desk_id": hex_desk})
    );
    assert_eq!(
        rt(&JoinPending {
            state: PendingState::Pending
        }),
        json!({"state":"pending"})
    );
    // desk.remove_member
    assert_eq!(
        rt(&RemoveMemberParams {
            computer_id: cid(3)
        }),
        json!({"computer_id": "03".repeat(16)})
    );
    // join.pending / join.decide
    let p = PendingJoin {
        request_id: 5,
        computer: ComputerRef {
            id: cid(3),
            name: "pc".into(),
        },
        desk: DeskRef {
            id: desk,
            name: "A".into(),
        },
    };
    assert_eq!(rt(&p)["desk"]["name"], "A");
    assert_eq!(
        rt(&JoinDecideParams {
            request_id: 5,
            approve: true
        }),
        json!({"request_id":5,"approve":true})
    );
    // layout.get/set
    let layout: LayoutResult = Layout::default();
    assert_eq!(rt(&layout)["block_fullscreen"], true);
    // config.get/set
    assert_eq!(rt(&ConfigGetParams { key: None }), json!({"key": null}));
    assert_eq!(
        rt(&ConfigSetParams {
            key: "advanced.port".into(),
            value: json!(1234)
        }),
        json!({"key":"advanced.port","value":1234})
    );
    // logging.set
    assert_eq!(
        rt(&LoggingSetParams { enabled: true }),
        json!({"enabled":true})
    );
    assert_eq!(
        rt(&LoggingSetResult { path: "/l".into() }),
        json!({"path":"/l"})
    );
    // empty results (pause/resume/leave/send_cad/...)
    assert_eq!(rt(&Empty::default()), json!({}));
    // update.*
    assert_eq!(
        rt(&UpdateResult {
            current: "1.0.0".into(),
            available: None
        }),
        json!({"current":"1.0.0","available":null})
    );
    // subscribe
    assert_eq!(
        rt(&SubscribeParams {
            topics: vec![Topic::Problem, Topic::Status]
        }),
        json!({"topics":["problem","status"]})
    );
}
