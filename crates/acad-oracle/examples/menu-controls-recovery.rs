//! Native evidence collector, not a compatibility test. Sole optional argument:
//! output directory. MENU_CONTROLS_CASES selects comma-separated case IDs.
//! At each review gate inspect the printed PNG, then enter an empty line;
//! EOF/anything else aborts rather than claiming a checked landing or prompt.
use acad_oracle::cga::Frame;
use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);
#[derive(Clone, Copy)]
enum Input {
    Line(&'static str),
    Click(usize, usize, &'static str),
    Move(usize, usize),
    Key(&'static str, bool),
    Control(&'static str),
}
use Input::{Click as C, Control as Ctrl, Key as K, Line as L, Move as M};
struct Case {
    id: &'static str,
    setup: Vec<Input>,
    probe: Vec<Input>,
    continuation: Vec<Input>,
    cleanup: Vec<Input>,
}
enum Outcome {
    Parsed,
    DecodeFailed(String),
}
fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut add = |id,
                   setup: Vec<Input>,
                   probe: Vec<Input>,
                   continuation: Vec<Input>,
                   cleanup: Vec<Input>| {
        out.push(Case {
            id,
            setup,
            probe,
            continuation,
            cleanup,
        });
    };
    let off = vec![
        L("SNAP"),
        L("0.5"),
        L("SNAP"),
        L("OFF"),
        L("ORTHO"),
        L("OFF"),
    ];
    add("TOPROW", vec![], vec![M(250, 4)], vec![], vec![]);
    add("BASEOFF", off.clone(), vec![], vec![], vec![]);
    let mut on = off.clone();
    on.extend([L("SNAP"), L("ON"), L("ORTHO"), L("ON")]);
    add("BASEON", on.clone(), vec![], vec![], vec![]);
    for (id, setup, row, label, n) in [
        ("SCIDLE", off.clone(), 12, "^Snap", 1),
        ("SCTWICE", off.clone(), 12, "^Snap", 2),
        ("SCON", on.clone(), 12, "^Snap", 1),
        ("OCIDLE", off.clone(), 20, "^Ortho", 1),
        ("OCTWICE", off.clone(), 20, "^Ortho", 2),
        ("OCON", on.clone(), 20, "^Ortho", 1),
    ] {
        add(id, setup, vec![C(600, row, label); n], vec![], vec![]);
    }
    let mut line = off.clone();
    line.extend([L("LINE"), L("2,3")]);
    add(
        "SLINE",
        line,
        vec![C(600, 12, "^Snap")],
        vec![L("4,5"), L("")],
        vec![],
    );
    for (id, row, label) in [("FSLINE", 12, "^Snap"), ("FOLINE", 20, "^Ortho")] {
        let mut setup = off.clone();
        setup.extend([L("LINE"), L("2.1,3.15")]);
        add(
            id,
            setup,
            vec![C(600, row, label)],
            vec![L("4.25,5.15"), L("")],
            vec![],
        );
    }
    add(
        "CMENU",
        vec![],
        vec![C(600, 4, "< GO >"), C(600, 156, "^Cancel")],
        vec![L("POINT"), L("8,7")],
        vec![],
    );
    for (id, row, label) in [("SCIRCLE", 12, "^Snap"), ("OCIRCLE", 20, "^Ortho")] {
        let mut setup = off.clone();
        setup.extend([L("CIRCLE"), L("2,3")]);
        add(id, setup, vec![C(600, row, label)], vec![L("1.25")], vec![]);
    }
    for (id, snap, ortho) in [
        ("M00", false, false),
        ("M10", true, false),
        ("M01", false, true),
        ("M11", true, true),
    ] {
        let mut setup = off.clone();
        if snap {
            setup.push(C(600, 12, "^Snap"));
        }
        if ortho {
            setup.push(C(600, 20, "^Ortho"));
        }
        setup.extend([L("LINE"), L("2,3")]);
        add(
            id,
            setup,
            vec![C(250, 90, "drawing off-grid/non-axis")],
            vec![L("")],
            vec![],
        );
    }
    let mut asymmetric = off.clone();
    asymmetric.extend([
        L("LINE"),
        L("2.1,3.15"),
        C(260, 85, "drawing unconstrained reference"),
        L(""),
        L("LINE"),
        L("2.1,3.15"),
        C(600, 12, "^Snap"),
        C(600, 20, "^Ortho"),
    ]);
    add(
        "MASYM",
        asymmetric,
        vec![C(260, 85, "drawing rounding/order discriminator")],
        vec![
            L(""),
            C(600, 12, "^Snap"),
            C(600, 20, "^Ortho"),
            L("LINE"),
            L("5.1,0.2"),
            C(260, 85, "drawing vertical unconstrained reference"),
            L(""),
            L("LINE"),
            L("5.1,0.2"),
            C(600, 12, "^Snap"),
            C(600, 20, "^Ortho"),
            C(260, 85, "drawing vertical rounding/order discriminator"),
            L(""),
        ],
        vec![],
    );
    let mut ortho_anchor = off.clone();
    ortho_anchor.extend([L("LINE"), L("2.1,3.15"), C(600, 20, "^Ortho")]);
    add(
        "MAORTHO",
        ortho_anchor,
        vec![C(250, 90, "drawing off-grid anchor discriminator")],
        vec![L("")],
        vec![],
    );
    let mut axis_pairs = Vec::new();
    for row in [60, 79, 74] {
        axis_pairs.extend([
            L("LINE"),
            C(200, 100, "drawing raw axis anchor"),
            C(260, row, "drawing raw axis endpoint"),
            L(""),
            L("LINE"),
            C(200, 100, "drawing matched axis anchor"),
            C(600, 20, "^Ortho"),
            C(260, row, "drawing constrained axis endpoint"),
            L(""),
            C(600, 20, "^Ortho"),
        ]);
    }
    add("MAXIS", off.clone(), axis_pairs, vec![], vec![]);
    for (id, setup) in [
        ("CIDLE", vec![]),
        ("CLSTART", vec![L("LINE")]),
        ("CLSEG", vec![L("LINE"), L("2,3")]),
        ("CLDONE", vec![L("LINE"), L("2,3"), L("4,5")]),
        ("CCENTER", vec![L("CIRCLE"), L("2,3")]),
    ] {
        add(
            id,
            setup,
            vec![C(600, 156, "^Cancel")],
            vec![L("POINT"), L("8,7")],
            vec![],
        );
    }
    // A known screen-space line makes the pending mouse selection observable
    // without estimating world coordinates from the viewport.
    add(
        "CSELECT",
        vec![
            L("LINE"),
            C(80, 100, "drawing seed"),
            C(480, 100, "drawing seed"),
            L(""),
            L("ERASE"),
            C(280, 100, "drawing selection"),
        ],
        vec![C(600, 156, "^Cancel")],
        vec![L("POINT"), L("8,7")],
        vec![],
    );
    add(
        "CSELBASE",
        vec![
            L("LINE"),
            C(80, 100, "drawing seed"),
            C(480, 100, "drawing seed"),
            L(""),
            L("ERASE"),
            C(280, 100, "drawing selection"),
        ],
        vec![L("")],
        vec![],
        vec![],
    );
    add(
        "GOBUFSEL",
        vec![
            L("LINE"),
            C(80, 100, "drawing seed"),
            C(480, 100, "drawing seed"),
            L(""),
            L("ERASE"),
            C(280, 100, "drawing selection"),
        ],
        vec![C(600, 4, "< GO >")],
        vec![L("POINT"), L("8,7")],
        vec![],
    );
    add(
        "GOBUFPNT",
        vec![
            L("LINE"),
            K("2", true),
            K("2", false),
            K("comma", true),
            K("comma", false),
            K("3", true),
            K("3", false),
        ],
        vec![C(600, 4, "< GO >")],
        vec![L("4,5"), L("")],
        vec![Ctrl("c")],
    );
    add(
        "CREPEAT",
        vec![L("REPEAT"), L("POINT"), L("4,5")],
        vec![C(600, 156, "^Cancel")],
        vec![L("ENDREP"), L("REPEAT"), L("POINT"), L("6,5"), L("ENDREP")],
        vec![Ctrl("c")],
    );
    for (id, setup, key) in [
        ("CBIDLE", vec![], "p"),
        ("CBPOINT", vec![L("LINE")], "2"),
        ("CBSELECT", vec![L("ERASE")], "1"),
    ] {
        let mut setup = setup;
        setup.extend([K(key, true), K(key, false)]);
        add(
            id,
            setup,
            vec![C(600, 156, "^Cancel")],
            vec![L("POINT"), L("8,7")],
            vec![],
        );
    }
    for (id, n) in [("CPAGE1", 1), ("CPAGE2", 2)] {
        let mut setup = vec![C(600, 164, "NEXT"); n];
        setup.push(L("LINE"));
        add(
            id,
            setup,
            vec![C(600, 156, "^Cancel")],
            vec![L("POINT"), L("8,7")],
            vec![],
        );
    }
    add(
        "GOIDLE",
        vec![M(100, 40), M(600, 4)],
        vec![C(600, 4, "< GO >")],
        vec![M(450, 130), C(600, 4, "< GO >")],
        vec![],
    );
    add(
        "GOFRESH",
        vec![M(100, 40)],
        vec![C(600, 4, "< GO >")],
        vec![M(450, 130), C(600, 4, "< GO >")],
        vec![],
    );
    add(
        "RTFRESH",
        vec![],
        vec![K("ret", true), K("ret", false)],
        vec![],
        vec![],
    );
    add(
        "GOHIST",
        vec![L("POINT"), L("8,7"), M(100, 40)],
        vec![C(600, 4, "< GO >")],
        vec![Ctrl("c"), M(450, 130), C(600, 4, "< GO >")],
        vec![Ctrl("c")],
    );
    add(
        "RETURN",
        vec![],
        vec![K("ret", true), K("ret", false)],
        vec![],
        vec![Ctrl("c")],
    );
    add(
        "SEMICOL",
        vec![],
        vec![K("semicolon", true), K("semicolon", false)],
        vec![K("ret", true), K("ret", false)],
        vec![],
    );
    for (id, setup, continuation) in [
        ("GOFIRST", vec![L("LINE")], vec![L("2,3"), L("4,5"), L("")]),
        ("GONEXT", vec![L("LINE"), L("2,3")], vec![L("4,5"), L("")]),
        ("GORADIUS", vec![L("CIRCLE"), L("2,3")], vec![L("1.25")]),
        (
            "GOSELECT",
            vec![L("LINE"), L("2,3"), L("4,5"), L(""), L("ERASE")],
            vec![],
        ),
    ] {
        add(
            id,
            setup,
            vec![C(600, 4, "< GO >")],
            continuation,
            vec![Ctrl("c")],
        );
    }
    for (id, n) in [("BLANK1", 1), ("BLANK2", 2)] {
        add(
            id,
            vec![C(600, 164, "NEXT"); n],
            vec![C(600, 4, "blank header")],
            vec![L("LINE"), L("2,3"), C(600, 4, "blank header")],
            vec![Ctrl("c")],
        );
    }
    add(
        "BLANKHI",
        vec![C(600, 164, "NEXT"), L("LINE"), L("2,3"), M(600, 156)],
        vec![C(600, 4, "blank header after hovering ^Cancel")],
        vec![L("POINT"), L("8,7")],
        vec![Ctrl("c")],
    );
    out
}
struct Recorder {
    root: PathBuf,
    transcript: File,
    case: String,
    seq: usize,
    phase: &'static str,
}
impl Recorder {
    fn log(&mut self, vm: &Session, action: &str, before: &str, after: &str) -> Result<(), String> {
        writeln!(
            self.transcript,
            "{}\t{}\t{}\t{}\t{:?}\t{}\t{}",
            self.case,
            self.seq,
            self.phase,
            action,
            vm.pointer(),
            before,
            after
        )
        .map_err(|e| e.to_string())?;
        self.transcript.flush().map_err(|e| e.to_string())
    }
    fn capture(&mut self, vm: &mut Session) -> Result<String, String> {
        self.seq += 1;
        let stem = format!("captures/{}-{:03}-{}", self.case, self.seq, self.phase);
        let raw = vm.capture_editor()?;
        fs::write(self.root.join(format!("{stem}.cga")), &raw).map_err(|e| e.to_string())?;
        let frame = Frame::new(&raw).map_err(|e| e.to_string())?;
        let mut pixmap = tiny_skia::Pixmap::new(640, 400).ok_or("pixmap allocation")?;
        for (pixel, rgb) in pixmap.pixels_mut().iter_mut().zip(frame.to_rgb_640x400()) {
            *pixel = tiny_skia::PremultipliedColorU8::from_rgba(
                (rgb >> 16) as u8,
                (rgb >> 8) as u8,
                rgb as u8,
                255,
            )
            .ok_or("pixel")?;
        }
        pixmap
            .save_png(self.root.join(format!("{stem}.png")))
            .map_err(|e| e.to_string())?;
        Ok(format!("{stem}.png"))
    }
    fn gate(&self, artifact: &str, reason: &str) -> Result<(), String> {
        println!(
            "REVIEW {}: {reason}\n{}",
            self.case,
            self.root.join(artifact).display()
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
            || !line.trim().is_empty()
        {
            return Err("review aborted (expected empty line)".into());
        }
        Ok(())
    }
    fn key(&mut self, vm: &mut Session, key: &str, down: bool) -> Result<(), String> {
        let before = self.capture(vm);
        // A release must still reach the guest if its preceding capture fails.
        if down {
            before.as_ref().map_err(Clone::clone)?;
        }
        vm.key(key, down)?;
        thread::sleep(Duration::from_millis(150));
        let after = self.capture(vm)?;
        self.log(vm, &format!("key {key} {down}"), &before?, &after)
    }
    fn input(&mut self, vm: &mut Session, input: Input) -> Result<(), String> {
        match input {
            L(line) => {
                let before = self.capture(vm)?;
                vm.type_line(line)?;
                let after = self.capture(vm)?;
                self.log(vm, &format!("line {line:?}"), &before, &after)?;
            }
            K(key, down) => self.key(vm, key, down)?,
            Ctrl(key) => {
                if let Err(error) = self.key(vm, "ctrl", true) {
                    let _ = vm.key("ctrl", false);
                    return Err(error);
                }
                let result = (|| {
                    self.key(vm, key, true)?;
                    self.key(vm, key, false)
                })();
                // Always release the modifier even if a capture failed.
                let release = self.key(vm, "ctrl", false);
                result?;
                release?;
            }
            M(col, row) => {
                let before = self.capture(vm)?;
                let (x, y) = device_for_pixel(col, row);
                vm.mouse_to(x, y, 0)?;
                let after = self.capture(vm)?;
                self.log(
                    vm,
                    &format!("mouse pixel={col},{row} device={x},{y} buttons=0"),
                    &before,
                    &after,
                )?;
            }
            C(col, row, label) => {
                self.input(vm, M(col, row))?;
                let before = self.capture(vm)?;
                self.gate(
                    &before,
                    &format!("verify label {label:?} at native pixel ({col},{row})"),
                )?;
                let (x, y) = device_for_pixel(col, row);
                vm.mouse_to(x, y, LEFT)?;
                thread::sleep(Duration::from_millis(300));
                let pressed = self.capture(vm); // release even if the capture fails
                let release = vm.mouse_to(x, y, 0);
                release?;
                let pressed = pressed?;
                self.log(
                    vm,
                    &format!("mouse pixel={col},{row} device={x},{y} buttons={LEFT}"),
                    &before,
                    &pressed,
                )?;
                thread::sleep(Duration::from_millis(300));
                let after = self.capture(vm)?;
                self.log(
                    vm,
                    &format!("mouse pixel={col},{row} device={x},{y} buttons=0"),
                    &pressed,
                    &after,
                )?;
                self.gate(
                    &after,
                    "read post-click prompt before any continuation/cleanup",
                )?;
            }
        }
        Ok(())
    }
    fn run_case(&mut self, disk: &Path, case: &Case) -> Result<Outcome, String> {
        self.case = case.id.into();
        self.seq = 0;
        self.phase = "boot";
        let mut vm = Session::boot_disposable(disk, None, &[])?;
        self.log(&vm, "boot_disposable private system.img", "", "")?;
        vm.wait_for_text("Enter selection:", WAIT)?;
        self.input(&mut vm, L("1"))?;
        vm.wait_for_text("Enter NAME of drawing:", WAIT)?;
        self.input(&mut vm, L(case.id))?;
        vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)?;
        thread::sleep(Duration::from_millis(500));
        self.phase = "setup";
        // Fresh-history comparisons intentionally retain the automatically
        // loaded shipped ACAD menu: typing MENU would change the last command.
        if !matches!(case.id, "GOFRESH" | "RTFRESH") {
            self.input(&mut vm, L("MENU"))?;
            self.input(&mut vm, L("ACAD"))?;
        }
        let before = self.capture(&mut vm)?;
        vm.mouse_pin()?;
        let after = self.capture(&mut vm)?;
        self.log(&vm, "mouse_pin", &before, &after)?;
        for (phase, inputs) in [
            ("setup", &case.setup),
            ("probe", &case.probe),
            ("continuation", &case.continuation),
            ("cleanup", &case.cleanup),
        ] {
            self.phase = phase;
            for &input in inputs {
                self.input(&mut vm, input)?;
            }
        }
        let command = self.capture(&mut vm)?;
        self.gate(
            &command,
            "confirm Command: before END (abort if active input)",
        )?;
        self.phase = "export";
        self.input(&mut vm, L("END"))?;
        vm.wait_for_text("Enter selection:", WAIT)?;
        self.input(&mut vm, L("5"))?;
        vm.wait_for_text("Enter NAME of drawing (default", WAIT)?;
        self.input(&mut vm, L(case.id))?;
        vm.wait_for_text("Drawing interchange file complete.", WAIT)?;
        vm.shutdown()?;
        self.log(&vm, "shutdown", "", "")?;
        let dwg = vm.read_system_file(&format!("{}.DWG", case.id))?;
        let dxf = vm.read_system_file(&format!("{}.DXF", case.id))?;
        fs::write(self.root.join(format!("drawings/{}.dwg", case.id)), &dwg)
            .map_err(|e| e.to_string())?;
        fs::write(self.root.join(format!("drawings/{}.dxf", case.id)), dxf)
            .map_err(|e| e.to_string())?;
        let mut hashes = String::new();
        for extension in ["dwg", "dxf"] {
            let path = self.root.join(format!("drawings/{}.{extension}", case.id));
            hashes.push_str(&output(
                "shasum",
                &["-a", "256", path.to_str().ok_or("artifact path encoding")?],
            )?);
            hashes.push('\n');
        }
        let hash_path = self.root.join(format!("drawings/{}.sha256", case.id));
        fs::write(&hash_path, &hashes).map_err(|e| e.to_string())?;
        let drawing = match acad_dwg::parse(&dwg) {
            Ok(drawing) => drawing,
            Err(error) => {
                let error = error.to_string();
                fs::write(
                    self.root
                        .join(format!("drawings/{}.parse-error.txt", case.id)),
                    &error,
                )
                .map_err(|e| e.to_string())?;
                println!("EXPORTED {}: DECODE FAILED: {error}", case.id);
                return Ok(Outcome::DecodeFailed(error));
            }
        };
        fs::write(
            self.root.join(format!("drawings/{}.decoded.txt", case.id)),
            format!(
                "Header: {:#?}\nItems: {:#?}\n",
                drawing.header, drawing.items
            ),
        )
        .map_err(|e| e.to_string())?;
        let path = self.root.join(format!("drawings/{}.decoded.txt", case.id));
        hashes.push_str(&output(
            "shasum",
            &["-a", "256", path.to_str().ok_or("artifact path encoding")?],
        )?);
        hashes.push('\n');
        fs::write(hash_path, hashes).map_err(|e| e.to_string())?;
        println!(
            "SAVED {} snap={:?} ortho={} items={:?}",
            case.id, drawing.header.snap, drawing.header.ortho, drawing.items
        );
        Ok(Outcome::Parsed)
    }
}
fn output(command: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(command)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "docs/recovery/2026-10-01-menu-controls".into()),
    );
    if args.next().is_some() {
        return Err("usage: menu-controls-recovery [evidence-directory]".into());
    }
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.is_file() || !acad_oracle::available() {
        return Err("missing System.img or qemu-system-i386; no observations collected".into());
    }
    if root.exists()
        && fs::read_dir(&root)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "refusing nonempty evidence directory {}",
            root.display()
        ));
    }
    let selected = std::env::var("MENU_CONTROLS_CASES").ok();
    let all = cases();
    if let Some(ids) = &selected {
        for id in ids.split(',') {
            if !all.iter().any(|case| case.id == id) {
                return Err(format!("unknown case {id}"));
            }
        }
    }
    fs::create_dir_all(root.join("captures")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("drawings")).map_err(|e| e.to_string())?;
    fs::write(root.join("provenance.txt"),format!("commit={}\nqemu={}\nsystem_sha256={}\nselected={}\nlaunch=-machine isapc -m 16 -d nochain; disposable copied floppy\n",output("git",&["rev-parse","HEAD"])?,output("qemu-system-i386",&["--version"])?,output("shasum",&["-a","256",disk.to_str().ok_or("image path encoding")?])?,selected.as_deref().unwrap_or("all"))).map_err(|e|e.to_string())?;
    let mut transcript = File::create(root.join("transcript.tsv")).map_err(|e| e.to_string())?;
    writeln!(
        transcript,
        "case\tsequence\tphase\tinput\tpointer_mirrored\tbefore_artifact\tafter_artifact"
    )
    .map_err(|e| e.to_string())?;
    let mut recorder = Recorder {
        root: root.clone(),
        transcript,
        case: String::new(),
        seq: 0,
        phase: "boot",
    };
    let mut decode_failures = Vec::new();
    for case in &all {
        if selected
            .as_ref()
            .is_some_and(|ids| !ids.split(',').any(|id| id == case.id))
        {
            continue;
        }
        match recorder.run_case(&disk, case) {
            Ok(Outcome::Parsed) => {}
            Ok(Outcome::DecodeFailed(error)) => {
                decode_failures.push(format!("{}\t{error}", case.id));
                fs::write(
                    root.join("decode-failures.tsv"),
                    format!("case\terror\n{}\n", decode_failures.join("\n")),
                )
                .map_err(|e| e.to_string())?;
            }
            Err(error) => {
                fs::write(root.join("failure.txt"), format!("{}: {error}\n", case.id))
                    .map_err(|e| e.to_string())?;
                let after = output(
                    "shasum",
                    &["-a", "256", disk.to_str().ok_or("image path encoding")?],
                )?;
                fs::write(root.join("system-after.sha256"), after).map_err(|e| e.to_string())?;
                return Err(format!("{}: {error}", case.id));
            }
        }
    }
    let after = output(
        "shasum",
        &["-a", "256", disk.to_str().ok_or("image path encoding")?],
    )?;
    fs::write(root.join("system-after.sha256"), after).map_err(|e| e.to_string())?;
    if decode_failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} native exports could not be decoded; see decode-failures.tsv (later cases retained)",
            decode_failures.len()
        ))
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("menu-controls-recovery: {error}");
        std::process::exit(1);
    }
}
