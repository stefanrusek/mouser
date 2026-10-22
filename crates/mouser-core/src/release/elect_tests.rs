use mouser_proto::ids::ComputerId;

use super::elect::elect;

fn id(n: u8) -> ComputerId {
    ComputerId([n; 16])
}

#[test]
fn the_lowest_id_among_the_online_members_is_the_fetcher() {
    assert_eq!(elect(id(5), [id(7), id(3), id(9)]), id(3));
    assert_eq!(elect(id(1), [id(7), id(3)]), id(1));
    assert_eq!(elect(id(5), []), id(5));
}

#[test]
fn the_order_members_are_listed_in_does_not_matter() {
    assert_eq!(elect(id(5), [id(3), id(9)]), elect(id(5), [id(9), id(3)]));
}
