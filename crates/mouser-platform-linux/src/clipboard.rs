//! How clipboard representations travel on Linux: which MIME type (X11
//! target) carries each [`ClipRep`], and how file lists are written as URIs.

use mouser_platform::ClipRep;
use std::path::PathBuf;

/// The X11 target for text.
pub(crate) const X11_TEXT: &str = "UTF8_STRING";
/// The Wayland MIME type for text.
pub(crate) const WAYLAND_TEXT: &str = "text/plain;charset=utf-8";
pub(crate) const HTML: &str = "text/html";
pub(crate) const RTF: &str = "text/rtf";
pub(crate) const PNG: &str = "image/png";
pub(crate) const URI_LIST: &str = "text/uri-list";
/// What file managers (Nautilus, Nemo, Caja, Dolphin) read to paste files.
pub(crate) const GNOME_FILES: &str = "x-special/gnome-copied-files";

/// Every type this backend reads or writes for `text`.
pub(crate) fn mime_types(text: &'static str) -> [&'static str; 6] {
    [text, HTML, RTF, PNG, URI_LIST, GNOME_FILES]
}

/// Bytes that stay as they are in a `file:` URI path.
fn plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte)
}

fn uri(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if plain(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn path_of(uri: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let encoded = uri.strip_prefix("file://")?.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < encoded.len() {
        if encoded[at] == b'%' {
            let hex = std::str::from_utf8(encoded.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(encoded[at]);
            at += 1;
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

/// `text/uri-list`: one URI per line, CRLF terminated (RFC 2483).
fn uri_list(paths: &[PathBuf]) -> Vec<u8> {
    paths
        .iter()
        .map(|p| format!("{}\r\n", uri(p)))
        .collect::<String>()
        .into_bytes()
}

/// The `x-special/gnome-copied-files` form: an action line, then the URIs.
fn gnome_files(paths: &[PathBuf]) -> Vec<u8> {
    let uris: Vec<String> = paths.iter().map(|p| uri(p)).collect();
    format!("copy\n{}", uris.join("\n")).into_bytes()
}

/// The paths in a URI list or gnome-copied-files body; lines that are not
/// `file:` URIs (comments, the action line, other schemes) are skipped.
fn paths(body: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(body)
        .lines()
        .filter_map(path_of)
        .collect()
}

/// Every type to offer for `reps`, with its bytes.
pub(crate) fn encode(text: &'static str, reps: &[ClipRep]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for rep in reps {
        match rep {
            ClipRep::Text(t) => out.push((text.to_string(), t.clone().into_bytes())),
            ClipRep::Html(h) => out.push((HTML.to_string(), h.clone().into_bytes())),
            ClipRep::Rtf(r) => out.push((RTF.to_string(), r.clone())),
            ClipRep::Png(p) => out.push((PNG.to_string(), p.clone())),
            ClipRep::Files(files) => {
                out.push((URI_LIST.to_string(), uri_list(files)));
                out.push((GNOME_FILES.to_string(), gnome_files(files)));
            }
        }
    }
    out
}

/// The representations in a clipboard: `get` returns the bytes of a type, or
/// `None` when the owner does not offer it.
pub(crate) fn decode<E>(
    text: &'static str,
    mut get: impl FnMut(&str) -> Result<Option<Vec<u8>>, E>,
) -> Result<Vec<ClipRep>, E> {
    let mut out = Vec::new();
    if let Some(bytes) = get(text)? {
        out.push(ClipRep::Text(String::from_utf8_lossy(&bytes).into_owned()));
    }
    if let Some(bytes) = get(HTML)? {
        out.push(ClipRep::Html(String::from_utf8_lossy(&bytes).into_owned()));
    }
    if let Some(bytes) = get(RTF)? {
        out.push(ClipRep::Rtf(bytes));
    }
    if let Some(bytes) = get(PNG)? {
        out.push(ClipRep::Png(bytes));
    }
    let files = match get(URI_LIST)? {
        Some(list) => Some(list),
        None => get(GNOME_FILES)?,
    };
    if let Some(body) = files {
        out.push(ClipRep::Files(paths(&body)));
    }
    Ok(out)
}
