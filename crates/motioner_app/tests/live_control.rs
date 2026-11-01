//! Live control (D-0024): start `preview --control`, drive it with `ctl`,
//! take a screenshot and check the state. Opens a real window, so it skips
//! (printing `[skip] ...`) when there is no GPU adapter or the window cannot
//! start, like the other GPU tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::time::{Duration, Instant};

use motioner_render::{FrameRenderer, RenderError};

const HELLO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

struct Editor {
    child: Child,
    dir: PathBuf,
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn ctl(endpoint: &Path, request: &str) -> String {
    let out = Process::new(env!("CARGO_BIN_EXE_motioner"))
        .args(["ctl", "--endpoint", endpoint.to_str().unwrap(), request])
        .output()
        .expect("run motioner ctl");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn ctl_drives_a_running_editor() {
    match FrameRenderer::new(64, 64) {
        Ok(_) => {}
        Err(RenderError::NoAdapter { message }) => {
            println!("[skip] no GPU adapter: {message}");
            return;
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    }
    let dir = std::env::temp_dir().join(format!("motioner-live-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("hello.motion");
    fs::write(&program, HELLO).unwrap();
    let endpoint = dir.join("control.txt");
    let child = Process::new(env!("CARGO_BIN_EXE_motioner"))
        .args([
            "preview",
            program.to_str().unwrap(),
            "--paused",
            "--window-size",
            "960x540",
            "--control-file",
            endpoint.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start motioner preview");
    let mut editor = Editor { child, dir };

    let started = Instant::now();
    while !endpoint.exists() {
        if let Ok(Some(status)) = editor.child.try_wait() {
            println!("[skip] the editor window could not start ({status})");
            return;
        }
        assert!(
            started.elapsed() < Duration::from_mins(1),
            "the control endpoint never appeared"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(ctl(&endpoint, "select(\"@288\")").contains("\"ok\":true"));
    assert!(ctl(&endpoint, "seek(30)").contains("\"result\":\"frame 30\""));
    assert!(ctl(&endpoint, "visible(\"@586/i0\", false)").contains("\"ok\":true"));
    let state = ctl(&endpoint, "state()");
    assert!(state.contains("\"frame\":30"), "{state}");
    assert!(state.contains("\"selected\":\"@288\""), "{state}");
    assert!(state.contains("\"undo_depth\":1"), "{state}");
    assert!(state.contains("\"playing\":false"), "{state}");

    let shot = editor.dir.join("shot.png");
    let reply = ctl(
        &endpoint,
        &format!("screenshot({:?})", shot.to_str().unwrap()),
    );
    if reply.contains("COPY_SRC") {
        println!("[skip] this adapter cannot read the window back: {reply}");
    } else {
        assert!(reply.contains("\"width\":960"), "{reply}");
        let png = fs::read(&shot).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    // Undo through ctl is the same undo a person gets with Ctrl+Z.
    assert!(ctl(&endpoint, "undo()").contains("\"result\":\"undone\""));
    let state = ctl(&endpoint, "state()");
    assert!(state.contains("\"undo_depth\":0"), "{state}");
    assert!(state.contains("\"redo_depth\":1"), "{state}");
    let error = ctl(&endpoint, "seek(\"x\")");
    assert!(error.contains("\"kind\":\"bad_argument\""), "{error}");

    assert!(ctl(&endpoint, "quit()").contains("\"ok\":true"));
    let closed = Instant::now();
    loop {
        if editor.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            closed.elapsed() < Duration::from_secs(20),
            "quit() did not close"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!endpoint.exists(), "the endpoint file is removed on exit");
}
