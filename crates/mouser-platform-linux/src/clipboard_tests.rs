use crate::clipboard::*;
use mouser_platform::ClipRep;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;

fn offered(text: &'static str, reps: &[ClipRep]) -> HashMap<String, Vec<u8>> {
    encode(text, reps).into_iter().collect()
}

fn read_back(text: &'static str, held: &HashMap<String, Vec<u8>>) -> Vec<ClipRep> {
    decode::<Infallible>(text, |mime| Ok(held.get(mime).cloned())).unwrap()
}

#[test]
fn every_text_like_and_image_representation_has_its_own_type() {
    let reps = vec![
        ClipRep::Text("héllo".into()),
        ClipRep::Html("<b>x</b>".into()),
        ClipRep::Rtf(b"{\\rtf1}".to_vec()),
        ClipRep::Png(vec![0x89, b'P', 0, 255]),
    ];
    let held = offered(X11_TEXT, &reps);
    assert_eq!(held["UTF8_STRING"], "héllo".as_bytes());
    assert_eq!(held["text/html"], b"<b>x</b>");
    assert_eq!(held["text/rtf"], b"{\\rtf1}");
    assert_eq!(held["image/png"], [0x89, b'P', 0, 255]);
    assert_eq!(read_back(X11_TEXT, &held), reps);
    let wayland = offered(WAYLAND_TEXT, &reps[..1]);
    assert!(wayland.contains_key("text/plain;charset=utf-8"));
    assert_eq!(read_back(WAYLAND_TEXT, &wayland), reps[..1]);
}

#[test]
fn files_are_offered_as_a_uri_list_and_as_gnome_copied_files() {
    let files = vec![
        PathBuf::from("/home/a/my file.txt"),
        PathBuf::from("/tmp/ü%1"),
    ];
    let held = offered(X11_TEXT, &[ClipRep::Files(files.clone())]);
    assert_eq!(
        held["text/uri-list"],
        b"file:///home/a/my%20file.txt\r\nfile:///tmp/%C3%BC%251\r\n"
    );
    assert_eq!(
        held["x-special/gnome-copied-files"],
        b"copy\nfile:///home/a/my%20file.txt\nfile:///tmp/%C3%BC%251"
    );
    assert_eq!(read_back(X11_TEXT, &held), [ClipRep::Files(files)]);
}

#[test]
fn files_are_read_from_gnome_copied_files_when_there_is_no_uri_list() {
    let held = HashMap::from([(
        "x-special/gnome-copied-files".to_string(),
        b"cut\nfile:///a/b\nhttps://example.org/x\n#comment\nfile:///c%20d".to_vec(),
    )]);
    assert_eq!(
        read_back(X11_TEXT, &held),
        [ClipRep::Files(vec!["/a/b".into(), "/c d".into()])]
    );
}

#[test]
fn a_uri_with_a_broken_escape_is_skipped() {
    let held = HashMap::from([(
        "text/uri-list".to_string(),
        b"file:///ok\r\nfile:///bad%zz\r\nfile:///cut%4".to_vec(),
    )]);
    assert_eq!(
        read_back(X11_TEXT, &held),
        [ClipRep::Files(vec!["/ok".into()])]
    );
}

#[test]
fn nothing_offered_is_an_empty_clipboard() {
    assert_eq!(read_back(X11_TEXT, &HashMap::new()), []);
}

#[test]
fn invalid_utf8_text_is_replaced_not_lost() {
    let held = HashMap::from([("UTF8_STRING".to_string(), vec![b'a', 0xff])]);
    assert_eq!(
        read_back(X11_TEXT, &held),
        [ClipRep::Text("a\u{fffd}".into())]
    );
}

#[test]
fn a_failing_owner_fails_the_read() {
    let result = decode(X11_TEXT, |_| Err::<Option<Vec<u8>>, _>("gone"));
    assert_eq!(result, Err("gone"));
}

#[test]
fn the_types_this_backend_understands_are_listed() {
    assert_eq!(mime_types(X11_TEXT)[0], "UTF8_STRING");
    assert!(mime_types(WAYLAND_TEXT).contains(&"x-special/gnome-copied-files"));
}
