use std::sync::Mutex;

use crate::args::{Entries, run};

#[derive(Default)]
struct Chosen(Mutex<Vec<&'static str>>);

impl Entries for Chosen {
    fn install(&self) -> i32 {
        self.0.lock().unwrap().push("install");
        10
    }
    fn uninstall(&self) -> i32 {
        self.0.lock().unwrap().push("uninstall");
        11
    }
    fn service(&self) -> i32 {
        self.0.lock().unwrap().push("service");
        12
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn install_uninstall_and_no_arguments_select_their_entries() {
    let chosen = Chosen::default();
    let mut stderr = Vec::new();
    assert_eq!(run(&args(&["--install"]), &chosen, &mut stderr), 10);
    assert_eq!(run(&args(&["--uninstall"]), &chosen, &mut stderr), 11);
    assert_eq!(run(&args(&[]), &chosen, &mut stderr), 12);
    assert_eq!(
        *chosen.0.lock().unwrap(),
        ["install", "uninstall", "service"]
    );
    assert!(stderr.is_empty());
}

#[test]
fn any_other_arguments_exit_3_and_say_what_is_accepted() {
    for other in [&["--help"][..], &["--install", "--uninstall"], &["install"]] {
        let chosen = Chosen::default();
        let mut stderr = Vec::new();
        assert_eq!(run(&args(other), &chosen, &mut stderr), 3, "{other:?}");
        assert!(chosen.0.lock().unwrap().is_empty());
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            "usage: mouser-service.exe [--install | --uninstall]\n"
        );
    }
}
