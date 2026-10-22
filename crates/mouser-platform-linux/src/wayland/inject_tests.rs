use super::inject::button_code;

#[test]
fn mouser_buttons_map_to_linux_button_codes() {
    assert_eq!(
        [1, 2, 3, 4, 5].map(button_code),
        [0x110, 0x111, 0x112, 0x113, 0x114]
    );
}
