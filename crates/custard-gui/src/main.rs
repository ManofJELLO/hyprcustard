//! hyprcustard — a Qt Quick theming GUI for Hyprland.

#![deny(missing_docs)]

pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

const HELP: &str = "\
hyprcustard - a theming GUI for Hyprland

Usage:
  hyprcustard                 open the app
  hyprcustard --print-state   print the whole state document as JSON
  hyprcustard --print-block   print the Lua that would be written
  hyprcustard --version       print the version
  hyprcustard --help          print this

custard-watch, installed beside it, keeps the desktop's colors matching the
wallpaper; it is started from the Colors page.
";

fn main() {
    // sudo re-runs this program with `--askpass` when a hyprpm operation needs
    // a password: it asks the window that started the operation, prints the
    // answer, and exits. It must come before anything that would put a second
    // copy of the interface on screen.
    // The socket variable is set only on the process a hyprpm job runs, so
    // anything started underneath one that is us is an askpass call, whether
    // or not the flag survived. Without this, sudo running this executable
    // directly would put a second interface on screen instead of answering.
    let args: Vec<String> = std::env::args().collect();
    let flagged = args.iter().position(|a| a == "--askpass");
    if flagged.is_some() || std::env::var_os("CUSTARD_ASKPASS_SOCKET").is_some() {
        let prompt = match flagged {
            Some(index) => args.get(index + 1).map(String::as_str),
            // sudo passes the prompt as the only argument.
            None => args.get(1).map(String::as_str),
        };
        std::process::exit(custard_core::hyprpm::askpass(prompt.unwrap_or("Password:")));
    }

    // Answered without opening a window, as a packaged binary should: a
    // `--version` that put the interface on screen would be a surprise. An
    // option this does not know is refused for the same reason; Qt's own
    // (`-platform`, `-style`) take one dash and pass through.
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--version" | "-V" => {
                println!("hyprcustard {}", custard_core::VERSION);
                return;
            }
            "--help" | "-h" => {
                print!("{HELP}");
                return;
            }
            "--print-state" | "--print-block" => {}
            other if other.starts_with("--") => {
                eprintln!("hyprcustard: unknown option {other}; see --help");
                std::process::exit(2);
            }
            _ => {}
        }
    }

    // Someone coming from hyprwindowshade-gui keeps their settings and themes.
    for path in custard_core::paths::adopt_legacy_config() {
        eprintln!(
            "hyprcustard: carried over {} from hyprwindowshade-gui",
            custard_core::paths::contract(&path)
        );
    }

    // `--print-state` loads everything and dumps the state document without
    // starting Qt. Useful for checking what the app sees on a machine where the
    // GUI will not start, and for testing in CI.
    if std::env::args().any(|a| a == "--print-state") {
        let session = custard_core::Session::load();
        println!("{:#}", session.state());
        return;
    }
    if std::env::args().any(|a| a == "--print-block") {
        let session = custard_core::Session::load();
        match session.preview() {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Every control in this app draws its own background and contents, so the
    // Qt Quick Controls style only decides the few parts Qt still draws — and
    // Basic is the one that does not impose colors of its own on them.
    //
    // This does not stand between the app and the desktop's colors: the
    // palette comes from the platform theme regardless of the style, and the
    // default theme reads it. Overriding this variable changes how those
    // remaining parts are drawn, not what color anything is.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    // Qt's Wayland text input logs a warning every time focus leaves a text
    // field ("Trying to disable … but 0x0 is focused") — a known Qt quirk,
    // harmless, and noise in the terminal. Off unless the user asks for
    // logging rules of their own.
    if std::env::var_os("QT_LOGGING_RULES").is_none() {
        std::env::set_var("QT_LOGGING_RULES", "qt.qpa.wayland.textinput=false");
    }

    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("hyprcustard"));
        app.set_organization_name(&QString::from("hyprcustard"));
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // Qt 6.5 added `:/qt/qml` to the default import path; on 6.4 the module's
        // own qmldir is never found there, so its QML components resolve as
        // "not a type" while its C++ types still work. Adding it explicitly is a
        // no-op on newer Qt.
        engine.as_mut().add_import_path(&QString::from("qrc:/qt/qml"));

        // Without this, a QML failure leaves an empty process with no window and
        // no explanation.
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                eprintln!(
                    "hyprcustard: the interface failed to load ({}). \
                     Qt Quick Controls may be missing — on Arch that is the \
                     qt6-declarative package.",
                    url
                );
                std::process::exit(1);
            })
            .release();

        let root = std::env::var("CUSTARD_ROOT_QML")
            .unwrap_or_else(|_| "qrc:/qt/qml/dev/hyprcustard/gui/qml/Main.qml".to_string());
        engine.load(&QUrl::from(&root));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
