//! None of these touch the real machine: the privileged step is a fake
//! `Admin` that either refuses or runs the root-side installer on
//! temporary directories.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mouser_platform_macos::login_agent::LoginAgent;

use crate::login_screen::{
    Admin, FLAG, Osascript, Outcome, Refused, allow, install_arg, install_as_root, shell_line,
};
use crate::support_tests::fixture;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn the_root_side_mode_is_selected_by_its_flag_and_names_the_staging_folder() {
    assert_eq!(
        install_arg(&args(&["mouser", FLAG, "/tmp/stage"])),
        Some(PathBuf::from("/tmp/stage"))
    );
    assert_eq!(install_arg(&args(&["mouser", "--headless"])), None);
    assert_eq!(install_arg(&args(&["mouser"])), None);
}

#[test]
fn the_shell_line_quotes_every_word() {
    assert_eq!(
        shell_line(
            Path::new("/Apps/My Mouser's/mouser"),
            &args(&[FLAG, "/tmp/a b"])
        ),
        "'/Apps/My Mouser'\\''s/mouser' '--install-login-screen' '/tmp/a b'"
    );
}

type Seen = Mutex<Vec<Vec<String>>>;

fn osascript(seen: &Seen, outcome: Outcome) -> Osascript<impl Fn(&[String]) -> Outcome + '_> {
    Osascript(move |argv: &[String]| {
        seen.lock().unwrap().push(argv.to_vec());
        outcome.clone()
    })
}

#[test]
fn osascript_runs_the_command_with_administrator_privileges() {
    let seen = Seen::default();
    let admin = osascript(
        &seen,
        Outcome {
            success: true,
            stderr: String::new(),
        },
    );
    admin
        .run(Path::new("/Apps/\"Mouser\""), &args(&[FLAG, "/tmp/s"]))
        .unwrap();
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![args(&[
            "-e",
            "do shell script \"'/Apps/\\\"Mouser\\\"' '--install-login-screen' '/tmp/s'\" with administrator privileges"
        ])]
    );
}

#[test]
fn cancelling_the_administrator_prompt_is_a_refusal() {
    let seen = Seen::default();
    let admin = osascript(
        &seen,
        Outcome {
            success: false,
            stderr: "execution error: User canceled. (-128)".into(),
        },
    );
    assert_eq!(admin.run(Path::new("/x"), &[]), Err(Refused));
}

#[test]
#[should_panic(expected = "the administrator step failed: boom")]
fn any_other_failure_of_the_administrator_step_is_loud() {
    let seen = Seen::default();
    let admin = osascript(
        &seen,
        Outcome {
            success: false,
            stderr: "boom".into(),
        },
    );
    let _ = admin.run(Path::new("/x"), &[]);
}

fn staged(dir: &Path) {
    std::fs::create_dir_all(dir.join("secrets")).unwrap();
    std::fs::write(dir.join("state.json"), "{}").unwrap();
    std::fs::write(dir.join("config.toml"), "# cfg").unwrap();
    std::fs::write(dir.join("secrets/device"), "key").unwrap();
}

#[test]
fn the_root_installer_copies_the_staged_files_and_writes_the_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let (stage, dest, agents) = (
        tmp.path().join("stage"),
        tmp.path().join("machine"),
        tmp.path().join("agents"),
    );
    staged(&stage);
    std::fs::create_dir_all(dest.join("secrets")).unwrap();
    std::fs::write(dest.join("secrets/stale"), "old").unwrap();
    std::fs::create_dir_all(&agents).unwrap();
    let agent = LoginAgent::new(
        &agents,
        "/Applications/Mouser.app/Contents/MacOS/mouser-app",
    );

    install_as_root(&stage, &dest, &agent);

    assert_eq!(
        std::fs::read_to_string(dest.join("state.json")).unwrap(),
        "{}"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("config.toml")).unwrap(),
        "# cfg"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("secrets/device")).unwrap(),
        "key"
    );
    assert!(!dest.join("secrets/stale").exists());
    assert!(agent.installed());
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dest), 0o700);
    assert_eq!(mode(&dest.join("secrets")), 0o700);
}

/// Stands in for the administrator prompt: approves by running the root-side
/// installer on temporary directories.
struct Approving {
    dest: PathBuf,
    agent: LoginAgent,
    ran: Mutex<Vec<(PathBuf, Vec<String>)>>,
}

impl Admin for Approving {
    fn run(&self, program: &Path, argv: &[String]) -> Result<(), Refused> {
        self.ran
            .lock()
            .unwrap()
            .push((program.to_path_buf(), argv.to_vec()));
        let stage = install_arg(argv).expect("the root mode is asked for");
        install_as_root(&stage, &self.dest, &self.agent);
        Ok(())
    }
}

struct Refusing(Mutex<usize>);

impl Admin for Refusing {
    fn run(&self, _: &Path, _: &[String]) -> Result<(), Refused> {
        *self.0.lock().unwrap() += 1;
        Err(Refused)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn allowing_exports_the_machine_scope_then_installs_through_one_prompt() {
    let f = fixture().await;
    let tmp = tempfile::tempdir().unwrap();
    let user_config = tmp.path().join("user");
    std::fs::create_dir_all(&user_config).unwrap();
    std::fs::write(user_config.join("config.toml"), "# mine").unwrap();
    let stage = tmp.path().join("stage");
    let agents = tmp.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    let exe = Path::new("/Applications/Mouser.app/Contents/MacOS/mouser-app");
    let admin = Approving {
        dest: tmp.path().join("machine"),
        agent: LoginAgent::new(&agents, exe),
        ran: Mutex::default(),
    };

    allow(&f.host, &user_config, &stage, exe, &admin).unwrap();

    assert_eq!(
        admin.ran.lock().unwrap().clone(),
        vec![(exe.to_path_buf(), args(&[FLAG, stage.to_str().unwrap()]))]
    );
    let machine = tmp.path().join("machine");
    assert_eq!(
        std::fs::read_to_string(machine.join("config.toml")).unwrap(),
        "# mine"
    );
    assert!(machine.join("state.json").exists());
    assert!(std::fs::read_dir(machine.join("secrets")).unwrap().count() > 0);
    assert!(admin.agent.installed());
    assert!(!stage.exists(), "the staged secrets are removed afterwards");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_user_config_file_none_is_staged() {
    let f = fixture().await;
    let tmp = tempfile::tempdir().unwrap();
    let agents = tmp.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    let exe = Path::new("/m");
    let admin = Approving {
        dest: tmp.path().join("machine"),
        agent: LoginAgent::new(&agents, exe),
        ran: Mutex::default(),
    };
    allow(&f.host, tmp.path(), &tmp.path().join("stage"), exe, &admin).unwrap();
    assert!(!tmp.path().join("machine/config.toml").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn refusing_the_prompt_changes_nothing_and_says_so() {
    let f = fixture().await;
    let tmp = tempfile::tempdir().unwrap();
    let stage = tmp.path().join("stage");
    let admin = Refusing(Mutex::new(0));

    let error = allow(&f.host, tmp.path(), &stage, Path::new("/m"), &admin).unwrap_err();

    assert_eq!(
        error,
        "Mouser was not given permission, so nothing was changed. Choose \"Allow control at the login screen\" again when you are ready to enter your password."
    );
    assert_eq!(*admin.0.lock().unwrap(), 1);
    assert!(!stage.exists(), "the staged secrets are removed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_leftover_staging_folder_is_replaced() {
    let f = fixture().await;
    let tmp = tempfile::tempdir().unwrap();
    let stage = tmp.path().join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("leftover"), "x").unwrap();
    let admin = Refusing(Mutex::new(0));
    let _ = allow(&f.host, tmp.path(), &stage, Path::new("/m"), &admin);
    assert!(!stage.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_settings_button_reaches_the_administrator_step_over_ipc() {
    use tauri::Manager;
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets};

    let f = fixture().await;
    let admin: Box<dyn Admin> = Box::new(Refusing(Mutex::new(0)));
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            crate::login_screen::allow_login_screen
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(f.host);
    app.manage(admin);
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let error = get_ipc_response::<&tauri::WebviewWindow<tauri::test::MockRuntime>>(
        &&window,
        crate::commands_tests::request("allow_login_screen", serde_json::json!({})),
    )
    .unwrap_err();
    assert!(
        error
            .as_str()
            .unwrap()
            .starts_with("Mouser was not given permission")
    );
}
