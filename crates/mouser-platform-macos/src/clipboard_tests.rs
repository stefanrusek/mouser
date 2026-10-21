use crate::clipboard::{
    Board, HTML, Item, MacClipboard, PNG, RTF, TEXT, normalise_path, read, write,
};
use mouser_platform::{ClipRep, Clipboard, Error};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

/// An in-memory pasteboard.
#[derive(Default)]
struct Fake {
    buffers: RefCell<Vec<(String, Vec<u8>)>>,
    files: RefCell<Vec<String>>,
    count: Cell<u64>,
    fail: bool,
}

impl Board for Fake {
    fn buffer(&self, format: &str) -> Option<Vec<u8>> {
        self.buffers
            .borrow()
            .iter()
            .find(|(f, _)| f == format)
            .map(|(_, b)| b.clone())
    }

    fn files(&self) -> Vec<String> {
        self.files.borrow().clone()
    }

    fn write(&self, items: Vec<Item>) -> mouser_platform::Result<()> {
        if self.fail {
            return Err(Error::Os("refused".into()));
        }
        self.buffers.borrow_mut().clear();
        self.files.borrow_mut().clear();
        for item in items {
            match item {
                Item::Buffer(f, b) => self.buffers.borrow_mut().push((f.to_string(), b)),
                Item::Files(f) => *self.files.borrow_mut() = f,
            }
        }
        self.count.set(self.count.get() + 1);
        Ok(())
    }

    fn change_count(&self) -> u64 {
        self.count.get()
    }
}

#[test]
fn an_empty_pasteboard_reads_as_no_representations() {
    assert_eq!(read(&Fake::default()), []);
}

#[test]
fn every_representation_written_together_is_read_back() {
    let board = Fake::default();
    let reps = vec![
        ClipRep::Text("hello".into()),
        ClipRep::Html("<b>hello</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 hello}".to_vec()),
        ClipRep::Png(vec![0x89, b'P', b'N', b'G']),
        ClipRep::Files(vec![PathBuf::from("/Users/a/one.txt")]),
    ];
    write(&board, reps.clone()).unwrap();
    assert_eq!(read(&board), reps);
}

#[test]
fn representations_use_the_standard_pasteboard_types() {
    let board = Fake::default();
    write(
        &board,
        vec![
            ClipRep::Text("t".into()),
            ClipRep::Html("h".into()),
            ClipRep::Rtf(b"r".to_vec()),
            ClipRep::Png(b"p".to_vec()),
        ],
    )
    .unwrap();
    assert_eq!(board.buffer(TEXT).unwrap(), b"t");
    assert_eq!(board.buffer(HTML).unwrap(), b"h");
    assert_eq!(board.buffer(RTF).unwrap(), b"r");
    assert_eq!(board.buffer(PNG).unwrap(), b"p");
}

#[test]
fn a_write_replaces_what_was_there() {
    let board = Fake::default();
    write(&board, vec![ClipRep::Text("old".into())]).unwrap();
    write(&board, vec![ClipRep::Html("new".into())]).unwrap();
    assert_eq!(read(&board), [ClipRep::Html("new".into())]);
}

#[test]
fn file_paths_are_normalised_when_read() {
    let board = Fake::default();
    *board.files.borrow_mut() = vec![
        "/private/var/folders/x/a.txt".into(),
        "file:///Users/a/My%20File.txt".into(),
    ];
    assert_eq!(
        read(&board),
        [ClipRep::Files(vec![
            PathBuf::from("/var/folders/x/a.txt"),
            PathBuf::from("/Users/a/My File.txt"),
        ])]
    );
}

#[test]
fn normalise_path_rules() {
    assert_eq!(normalise_path("/Users/a/b"), PathBuf::from("/Users/a/b"));
    assert_eq!(normalise_path("/private/tmp"), PathBuf::from("/tmp"));
    assert_eq!(
        normalise_path("/private/etc/hosts"),
        PathBuf::from("/etc/hosts")
    );
    // Only whole components are rewritten.
    assert_eq!(
        normalise_path("/private/variable"),
        PathBuf::from("/private/variable")
    );
    assert_eq!(
        normalise_path("/private/other"),
        PathBuf::from("/private/other")
    );
    assert_eq!(
        normalise_path("file://localhost/private/var/a%2Fb"),
        PathBuf::from("/var/a/b")
    );
    // Malformed escapes are kept.
    assert_eq!(normalise_path("file:///a%zz%4"), PathBuf::from("/a%zz%4"));
}

#[test]
fn change_count_follows_the_board() {
    let mut clipboard = MacClipboard::new(Fake::default());
    let before = clipboard.change_count();
    clipboard.write(vec![ClipRep::Text("x".into())]).unwrap();
    assert!(clipboard.change_count() > before);
    assert_eq!(clipboard.read().unwrap(), [ClipRep::Text("x".into())]);
}

#[test]
fn a_refused_write_is_an_error() {
    let mut clipboard = MacClipboard::new(Fake {
        fail: true,
        ..Fake::default()
    });
    assert_eq!(
        clipboard.write(vec![ClipRep::Text("x".into())]),
        Err(Error::Os("refused".into()))
    );
}
