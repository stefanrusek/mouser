use crate::DiagValue;

#[test]
fn values_read_as_they_are_written() {
    assert_eq!(DiagValue::Str("game.exe".into()).to_string(), "game.exe");
    assert_eq!(DiagValue::Bool(false).to_string(), "false");
    assert_eq!(DiagValue::Int(3).to_string(), "3");
}
