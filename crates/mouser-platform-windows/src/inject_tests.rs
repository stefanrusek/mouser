use crate::inject::*;
use mouser_platform::{Error, InputInject};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_WHEEL, VK_F24, VK_XBUTTON1,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

fn cursor() -> (i32, i32) {
    crate::dpi::physical(|| {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p).unwrap() };
        (p.x, p.y)
    })
}

fn held(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
    state < 0
}

#[test]
fn key_events_are_scan_codes_with_extended_and_release_flags() {
    let k = unsafe { key_input(0x4D, true, false, MOUSER_TAG).Anonymous.ki };
    assert_eq!(k.wScan, 0x4D);
    assert_eq!(k.dwExtraInfo, MOUSER_TAG);
    assert_eq!(
        k.dwFlags,
        KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP
    );
    let k = unsafe { key_input(0x1E, false, true, MOUSER_TAG).Anonymous.ki };
    assert_eq!(k.dwFlags, KEYEVENTF_SCANCODE);
}

#[test]
fn mouse_events_carry_their_data_and_tag() {
    let m = unsafe {
        mouse_input(MOUSEEVENTF_WHEEL, -120, MOUSER_TAG)
            .Anonymous
            .mi
    };
    assert_eq!(m.mouseData as i32, -120);
    assert_eq!(m.dwExtraInfo, MOUSER_TAG);
}

#[test]
fn a_failed_call_is_an_os_error_that_names_it() {
    assert_eq!(os("Call", Ok(())), Ok(()));
    let Err(Error::Os(text)) = os::<()>(
        "Call",
        Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            -1,
        ))),
    ) else {
        panic!("expected an error")
    };
    assert!(text.starts_with("Call: "), "{text}");
}

#[test]
fn a_partly_accepted_batch_is_an_error() {
    assert_eq!(check_sent(2, 2), Ok(()));
    assert_eq!(
        check_sent(0, 1),
        Err(Error::Os("SendInput accepted 0 of 1 events".into()))
    );
}

#[test]
fn absolute_moves_use_desktop_logical_coordinates() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    let before = cursor();
    let all = crate::displays::displays().unwrap();
    let primary = all.iter().find(|d| d.origin == (0.0, 0.0)).unwrap();
    // The centre of the primary display, in logical points.
    let (x, y) = (primary.logical_size.0 / 2.0, primary.logical_size.1 / 2.0);
    inject.motion_abs(x, y).unwrap();
    let scale = f64::from(primary.scale);
    assert_eq!(
        cursor(),
        ((x * scale).round() as i32, (y * scale).round() as i32)
    );
    crate::dpi::physical(|| unsafe { SetCursorPos(before.0, before.1).unwrap() });
}

#[test]
fn buttons_one_to_five_and_unknown_buttons() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    // Only the X buttons are safe to send to whatever is under the pointer:
    // a bare back/forward press and release is the worst that can happen.
    for button in [4, 5] {
        inject.button(button, true).unwrap();
        inject.button(button, false).unwrap();
    }
    assert_eq!(
        inject.button(6, true),
        Err(Error::Os("mouse button 6 does not exist".into()))
    );
    assert_eq!(
        inject.button(0, false),
        Err(Error::Os("mouse button 0 does not exist".into()))
    );
}

#[test]
fn the_left_middle_and_right_events_exist() {
    for button in 1..=3 {
        for down in [true, false] {
            assert!(button_event(button, down).is_ok());
        }
    }
}

#[test]
fn scroll_sends_only_the_axes_that_moved() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    inject.scroll(0.0, 0.0).unwrap();
    inject.scroll(0.0, 1.0).unwrap();
    inject.scroll(1.0, 0.0).unwrap();
    inject.scroll(-1.0, -1.0).unwrap();
}

#[test]
fn keys_without_a_scan_code_are_dropped() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    // Pause, and a usage on a page Mouser does not forward.
    inject.key(0x0007_0048, true).unwrap();
    inject.key(0x0001_0004, true).unwrap();
}

#[test]
fn what_is_still_held_is_released_on_drop() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    // F24 is Mouser's own scratch key; X button 1 is back.
    inject.key(0x0007_0073, true).unwrap();
    inject.button(4, true).unwrap();
    assert!(held(VK_F24) && held(VK_XBUTTON1));
    drop(inject);
    assert!(!held(VK_F24) && !held(VK_XBUTTON1));
}

#[test]
fn released_input_is_not_released_again() {
    let _live = crate::live_input();
    let mut inject = WinInject::new(None);
    inject.key(0x0007_0073, true).unwrap();
    inject.key(0x0007_0073, false).unwrap();
    inject.button(5, true).unwrap();
    inject.button(5, false).unwrap();
    assert!(inject.keys.is_empty() && inject.buttons.is_empty());
}

#[test]
fn secure_attention_goes_to_the_service_when_there_is_one() {
    let mut without = WinInject::new(None);
    assert_eq!(without.secure_attention(), Err(Error::Unsupported));
    let mut with = WinInject::new(Some(Box::new(|| Ok(()))));
    assert_eq!(with.secure_attention(), Ok(()));
}

#[test]
fn a_platform_with_the_service_asks_it_for_ctrl_alt_del_and_maps_its_answer() {
    use crate::service_client_tests::{serve, unique_name};
    use mouser_platform::Platform;
    use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage};
    let name = unique_name();
    let answers = [
        SasResult::Sent,
        SasResult::PolicyBlocked,
        SasResult::Failed("refused".into()),
    ];
    let replies = answers.clone();
    let service = serve(&name, move |end| {
        end.read();
        let mut asked = Vec::new();
        for answer in replies {
            asked.push(end.read());
            end.send(&ServiceMessage::SasResult(answer));
        }
        asked
    });
    let client = crate::service_client::connect_to(&name, "0.3.0").unwrap();
    let platform = crate::WinPlatform::with_service_client(crate::Settings::system(), client);
    let mut inject = platform.inject().unwrap();
    let results: Vec<_> = answers.iter().map(|_| inject.secure_attention()).collect();
    let asked = service.join().unwrap();
    drop(inject);
    drop(platform);
    assert_eq!(asked, vec![Some(EngineMessage::SendSas); 3]);
    assert_eq!(results[0], Ok(()));
    assert!(matches!(results[1], Err(Error::Os(_))), "{:?}", results[1]);
    assert!(matches!(results[2], Err(Error::Os(_))), "{:?}", results[2]);
}

#[test]
fn a_platform_without_the_service_keeps_ctrl_alt_del_unsupported() {
    use mouser_platform::Platform;
    let mut inject = crate::WinPlatform::new().inject().unwrap();
    assert_eq!(inject.secure_attention(), Err(Error::Unsupported));
}
