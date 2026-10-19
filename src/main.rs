//! vista: a living pixel landscape that greets you when a terminal opens.
//!
//! Fog lifts off the mountains, the sun or moon sits where the real clock
//! says it should, stars twinkle at night and fireflies come out at dusk.
//! Next to the picture, an info panel tinted with the colours of the sky.

mod scene;
mod setup;
mod store;
mod sys;

use scene::{fg, hex, mix, Phase, Rgb};
use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FOG_SECS: f32 = 1.5;
const END_SECS: f32 = 1.65;
const FRAME: Duration = Duration::from_millis(33);
/// Reopening within this many seconds shows the final frame without fog.
const QUICK_REOPEN_SECS: u64 = 600;
/// Default picture size, percent of the full 96x60.
const DEFAULT_SIZE: u32 = 50;
const GAP: usize = 4;
const PANEL_W: usize = 40;
const PANEL_LINES: usize = 12;
const BAR_CELLS: usize = 10;

const TEXT: Rgb = hex(0xe5e7eb);
const DIM: Rgb = hex(0x7c8594);
const FAINT: Rgb = hex(0x3a3f4b);
const WARN: Rgb = hex(0xf87171);
const BOLT: Rgb = hex(0xfacc15);
const FIREFLY: Rgb = hex(0xfde047);

struct Args {
    still: bool,
    animate: bool,
    demo: bool,
    save: bool,
    cols: Option<u16>,
    rows: Option<u16>,
    time: Option<f32>,
    name: Option<String>,
    size: Option<u32>,
    ppm: Option<String>,
}

const HELP: &str = "vista - a living pixel landscape for your terminal

Usage: vista [options]
       vista init [--name NAME]   run vista in every new terminal tab
       vista init SHELL           print the lines for one shell (powershell, cmd, bash, zsh)
       vista uninstall            remove vista from your shells and delete its settings

Options:
  --static         show the final frame, no animation
  --animate        always animate, even on a quick reopen
  --demo           play dawn, noon, sunset and night in a row
  --time HH:MM     pretend it is this time (preview the sky)
  --name NAME      name used in the greeting
  --size PERCENT   picture size, 20-100 (default 50)
  --cols N         terminal width (used from WSL)
  --rows N         terminal height (used from WSL)
  --no-save        do not record this opening
  --ppm FILE       save the final picture as a PPM image

Config:  %APPDATA%\\vista\\config.txt   (name=Sam, size=50)
Env:     VISTA_NAME=Sam   VISTA_OFF=1 to turn it off";

/// `vista init ...`: set up the shells, or print one shell's lines.
fn init(args: &[String]) -> i32 {
    match args {
        [] => setup::init(None),
        [flag, name] if flag == "--name" => setup::init(Some(name.clone())),
        [shell] => match setup::Shell::parse(shell) {
            Some(s) => {
                setup::print(s);
                0
            }
            None => {
                eprintln!("vista: unknown shell '{shell}' (powershell, cmd, bash or zsh)");
                2
            }
        },
        _ => {
            eprintln!("vista: usage: vista init [--name NAME] | vista init SHELL");
            2
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args {
        still: false,
        animate: false,
        demo: false,
        save: true,
        cols: None,
        rows: None,
        time: None,
        name: None,
        size: None,
        ppm: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--static" => a.still = true,
            "--animate" => a.animate = true,
            "--demo" => {
                a.demo = true;
                a.save = false;
            }
            "--no-save" => a.save = false,
            "--cols" => a.cols = it.next().and_then(|v| v.parse().ok()),
            "--rows" => a.rows = it.next().and_then(|v| v.parse().ok()),
            "--time" => a.time = it.next().and_then(|v| parse_hhmm(&v)),
            "--name" => a.name = it.next(),
            "--size" => a.size = it.next().and_then(|v| v.trim_end_matches('%').parse().ok()),
            "--ppm" => a.ppm = it.next(),
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => {
                eprintln!("vista: unknown option '{other}' (try --help)");
                std::process::exit(2);
            }
        }
    }
    a
}

fn parse_hhmm(s: &str) -> Option<f32> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (f32, f32) = (h.parse().ok()?, m.parse().ok()?);
    ((0.0..24.0).contains(&h) && (0.0..60.0).contains(&m)).then_some(h + m / 60.0)
}

fn user_name(arg: Option<String>) -> String {
    let pick = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(n) = pick(arg).or_else(|| pick(std::env::var("VISTA_NAME").ok())).or_else(|| store::config("name")) {
        return n;
    }
    let raw = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let first = raw.split_whitespace().next().unwrap_or("friend");
    let mut c = first.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "friend".into(),
    }
}

// ---------------------------------------------------------------- panel text

fn greeting(hr: f32, name: &str) -> String {
    match hr as u32 {
        5..=11 => format!("Good morning, {name}"),
        12..=16 => format!("Good afternoon, {name}"),
        17..=21 => format!("Good evening, {name}"),
        _ => format!("Still up, {name}?"),
    }
}

fn caption(ph: Phase, hr: f32, seed: u32) -> &'static str {
    let opts: [&str; 2] = match ph {
        Phase::Dusk if hr < 12.0 => ["Fog is lifting off the peaks.", "First light on the summit."],
        Phase::Dusk => ["The sun slips behind the ridge.", "Fireflies are waking up."],
        Phase::Day => ["Clear skies over the ridge.", "Not a cloud over the summit."],
        Phase::Night => ["Stars out, fireflies dancing.", "The valley is fast asleep."],
    };
    opts[(seed % 2) as usize]
}

/// Two accent colours per phase, picked from the sky in the picture.
fn accents(ph: Phase) -> (Rgb, Rgb) {
    match ph {
        Phase::Dusk => (hex(0xc084fc), hex(0xf0a37a)),
        Phase::Day => (hex(0x60a5fa), hex(0x6ee7b7)),
        Phase::Night => (hex(0x818cf8), hex(0xc4b5fd)),
    }
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn duration_text(mins: u32) -> String {
    let (h, m) = (mins / 60, mins % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn uptime_text(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs / 3600) % 24, (secs / 60) % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else {
        format!("{h}h {m}m")
    }
}

/// ("sunset", minutes) while the sun is up, ("sunrise", minutes) otherwise.
fn sun_event(hr: f32) -> (&'static str, u32) {
    let now = (hr * 60.0).round() as u32;
    if (360..1080).contains(&now) {
        ("sunset", 1080 - now)
    } else {
        ("sunrise", (360 + 1440 - now) % 1440)
    }
}

fn ease_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

fn gradient_text(s: &str, a: Rgb, b: Rgb, shown: usize) -> String {
    let n = s.chars().count().max(2);
    let mut out = String::new();
    for (i, ch) in s.chars().take(shown).enumerate() {
        out.push_str(&fg(mix(a, b, i as f32 / (n - 1) as f32)));
        out.push(ch);
    }
    out
}

fn bar(frac: f32, a: Rgb, b: Rgb, warn: bool) -> String {
    let filled = (frac.clamp(0.0, 1.0) * BAR_CELLS as f32).round() as usize;
    let mut out = String::new();
    for i in 0..BAR_CELLS {
        if i < filled {
            let c = if warn { WARN } else { mix(a, b, i as f32 / (BAR_CELLS - 1) as f32) };
            out.push_str(&fg(c));
            out.push('▰');
        } else {
            out.push_str(&fg(FAINT));
            out.push('▱');
        }
    }
    out
}

struct Info {
    name: String,
    hr: f32,
    now: sys::LocalTime,
    battery: Option<sys::Battery>,
    ram: u32,
    uptime: u64,
}

/// The info panel at time `t`. Always PANEL_LINES long; lines not revealed
/// yet are empty so the layout never jumps.
fn panel(t: f32, info: &Info, cpu: Option<u32>) -> Vec<String> {
    let ph = scene::phase(info.hr);
    let (a, b) = accents(ph);
    let icon = mix(a, b, 0.5);
    let mut lines = vec![String::new(); PANEL_LINES];

    // Greeting, typed in with a gradient
    let hello = greeting(info.hr, &info.name);
    let typed = (hello.chars().count() as f32 * ((t - 0.1) / 0.55).clamp(0.0, 1.0)).ceil() as usize;
    lines[0] = format!("\x1b[1m{}\x1b[22m", gradient_text(&hello, a, b, typed));

    let n = &info.now;
    let hh = info.hr as u32;
    let mm = ((info.hr - hh as f32) * 60.0).round() as u32 % 60;
    if t >= 0.5 {
        lines[1] = format!(
            "{}{}{}, {} {}  ·  {hh:02}:{mm:02}",
            fg(TEXT),
            DAYS[(n.weekday % 7) as usize],
            fg(DIM),
            MONTHS[((n.month + 11) % 12) as usize],
            n.day
        );
    }

    // Stats: each line slides in, bars fill and numbers count up
    let stat = |appear: f32, ico: char, label: &str, body: String| -> Option<String> {
        (t >= appear).then(|| format!("{}{ico}  {}{label:<8}{body}", fg(icon), fg(DIM)))
    };
    let meter = |appear: f32, value: u32, warn: bool| -> (String, u32) {
        let p = ease_out((t - appear) / 0.45);
        (bar(value as f32 / 100.0 * p, a, b, warn), (value as f32 * p).round() as u32)
    };

    let mut row = 3;
    let mut appear = 0.7;
    let mut push = |lines: &mut Vec<String>, l: Option<String>| {
        if let Some(l) = l {
            lines[row] = l;
        }
        row += 1;
    };

    match &info.battery {
        Some(bat) => {
            let warn = !bat.charging && bat.percent < 20;
            let (bar, v) = meter(appear, bat.percent as u32, warn);
            let ico = match bat.percent {
                80.. => '\u{f240}',
                55..=79 => '\u{f241}',
                30..=54 => '\u{f242}',
                10..=29 => '\u{f243}',
                _ => '\u{f244}',
            };
            let bolt = if bat.charging { format!(" {}\u{f0e7}", fg(BOLT)) } else { String::new() };
            let vc = if warn { WARN } else { TEXT };
            push(&mut lines, stat(appear, ico, "battery", format!("{bar} {}{v:>3}%{bolt}", fg(vc))));
        }
        None => push(&mut lines, stat(appear, '\u{f1e6}', "power", format!("{}plugged in", fg(TEXT)))),
    }
    appear += 0.08;
    match cpu {
        Some(c) => {
            let (bar, v) = meter(appear, c, false);
            push(&mut lines, stat(appear, '\u{f4bc}', "cpu", format!("{bar} {}{v:>3}%", fg(TEXT))));
        }
        None => push(&mut lines, None),
    }
    appear += 0.08;
    let (mbar, mv) = meter(appear, info.ram, info.ram >= 90);
    push(&mut lines, stat(appear, '\u{f2db}', "memory", format!("{mbar} {}{mv:>3}%", fg(TEXT))));
    appear += 0.08;
    push(&mut lines, stat(appear, '\u{f252}', "uptime", format!("{}{}", fg(TEXT), uptime_text(info.uptime))));
    appear += 0.08;
    let (event, mins) = sun_event(info.hr);
    let sun_ico = if event == "sunset" { '\u{f185}' } else { '\u{f186}' };
    push(&mut lines, stat(appear, sun_ico, event, format!("{}in {}", fg(TEXT), duration_text(mins))));

    // Caption, typed in italics
    let cap = caption(ph, info.hr, n.day + n.hour);
    let shown = (cap.chars().count() as f32 * ((t - 1.1) / 0.4).clamp(0.0, 1.0)).ceil() as usize;
    if shown > 0 {
        lines[9] = format!("\x1b[3m{}{}\x1b[23m", fg(DIM), cap.chars().take(shown).collect::<String>());
    }

    // Palette strip: the colours of this very picture, popping in one by one
    let pal = scene::palette(ph);
    let swatches = [
        pal.sky_top,
        mix(pal.sky_top, pal.sky_bottom, 0.5),
        pal.sky_bottom,
        pal.rock,
        pal.hill,
        pal.pine,
        pal.snow,
        pal.orb,
        FIREFLY,
    ];
    let count = (((t - 1.25) / 0.03).floor().max(0.0) as usize).min(swatches.len());
    if count > 0 {
        lines[11] = swatches[..count].iter().map(|c| format!("{}██", fg(*c))).collect();
    }
    lines
}

// ---------------------------------------------------------------- layout

#[derive(Clone, Copy, PartialEq, Debug)]
enum Layout {
    /// Picture on the left, panel on the right
    Side(usize, usize),
    /// Picture above the panel (narrow terminals)
    Stack(usize, usize),
    TextOnly,
}

fn picture_size(scale: f32) -> (usize, usize) {
    let tw = (scene::W as f32 * scale).round() as usize;
    let th = ((scene::H as f32 * scale).round() as usize) & !1;
    (tw, th)
}

fn layout(cols: u16, rows: u16, size: u32) -> Layout {
    let (cols, rows) = (cols as f32, rows as f32);
    let want = (size.clamp(20, 100) as f32) / 100.0;
    let w = scene::W as f32;
    let h = scene::H as f32;

    let side = want.min((cols - (GAP + PANEL_W) as f32) / w).min((rows - 2.0) * 2.0 / h);
    let (tw, th) = picture_size(side.max(0.0));
    // Side by side only while the picture stays at least a third of full width
    if tw >= 32 && th >= 16 {
        return Layout::Side(tw, th);
    }
    let stack = want.min(cols / w).min((rows - 3.0 - PANEL_LINES as f32) * 2.0 / h);
    let (tw, th) = picture_size(stack.max(0.0));
    if tw >= 24 && th >= 16 {
        return Layout::Stack(tw, th);
    }
    Layout::TextOnly
}

fn height(l: Layout) -> usize {
    match l {
        Layout::Side(_, th) => (th / 2).max(PANEL_LINES),
        Layout::Stack(_, th) => th / 2 + 1 + PANEL_LINES,
        Layout::TextOnly => PANEL_LINES,
    }
}

fn compose(t: f32, info: &Info, l: Layout, cpu: Option<u32>) -> Vec<String> {
    let side = panel(t, info, cpu);
    let picture = |tw: usize, th: usize| scene::to_lines(&scene::render(t, info.hr, FOG_SECS, tw, th));
    let mut out = Vec::new();
    match l {
        Layout::Side(tw, th) => {
            let img = picture(tw, th);
            let total = height(l);
            let img_off = (total - img.len()) / 2;
            let pan_off = (total - side.len()) / 2;
            for r in 0..total {
                let left = r.checked_sub(img_off).and_then(|i| img.get(i)).cloned().unwrap_or_else(|| " ".repeat(tw));
                let right = r.checked_sub(pan_off).and_then(|i| side.get(i)).cloned().unwrap_or_default();
                out.push(format!("{left}\x1b[0m{}{right}", " ".repeat(GAP)));
            }
        }
        Layout::Stack(tw, th) => {
            out.extend(picture(tw, th));
            out.push(String::new());
            out.extend(side.into_iter().map(|s| format!(" {s}")));
        }
        Layout::TextOnly => out.extend(side.into_iter().map(|s| format!(" {s}"))),
    }
    out.into_iter().map(|s| format!("\x1b[2K{s}\x1b[0m")).collect()
}

fn play(info: &Info, l: Layout, animate: bool) {
    let cpu = sys::CpuSampler::start();
    let mut out = std::io::stdout().lock();

    if !animate {
        std::thread::sleep(Duration::from_millis(120));
        let lines = compose(END_SECS, info, l, cpu.usage());
        let _ = write!(out, "{}\r\n", lines.join("\r\n"));
        let _ = out.flush();
        return;
    }

    let up = height(l) - 1;
    let _ = write!(out, "\x1b[?25l{}\x1b[{up}A\r", "\n".repeat(up));
    let start = Instant::now();
    let mut usage: Option<u32> = None;
    loop {
        let t = start.elapsed().as_secs_f32();
        if usage.is_none() && t >= 0.35 {
            usage = cpu.usage();
        }
        let done = t >= END_SECS;
        let lines = compose(t.min(END_SECS), info, l, usage);
        let tail = if done { "\r\n\x1b[?25h".to_string() } else { format!("\x1b[{up}A\r") };
        let _ = write!(out, "{}{tail}", lines.join("\r\n"));
        let _ = out.flush();
        if done {
            break;
        }
        std::thread::sleep(FRAME);
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match argv.first().map(String::as_str) {
        Some("init") => std::process::exit(init(&argv[1..])),
        Some("uninstall") => std::process::exit(setup::uninstall()),
        _ => {}
    }
    if std::env::var("VISTA_OFF").is_ok_and(|v| !v.is_empty() && v != "0") {
        return;
    }
    let args = parse_args();
    sys::prepare_console();

    let console = sys::console_size();
    let env_num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u16>().ok());
    let cols = args.cols.or_else(|| env_num("COLUMNS")).or(console.map(|c| c.0)).unwrap_or(120);
    let rows = args.rows.or_else(|| env_num("LINES")).or(console.map(|c| c.1)).unwrap_or(40);
    let size = args
        .size
        .or_else(|| store::config("size").and_then(|v| v.trim_end_matches('%').parse().ok()))
        .unwrap_or(DEFAULT_SIZE);
    let l = layout(cols, rows, size);

    let now = sys::local_time();
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let since_last = store::touch_last_open(unix, args.save);

    let mut info = Info {
        name: user_name(args.name.clone()),
        hr: args.time.unwrap_or(now.hour as f32 + now.minute as f32 / 60.0),
        battery: sys::battery(),
        ram: sys::ram_percent(),
        uptime: sys::uptime_secs(),
        now,
    };

    if let Some(path) = &args.ppm {
        let (tw, th) = picture_size(size.clamp(20, 100) as f32 / 100.0);
        let img = scene::render(END_SECS, info.hr, FOG_SECS, tw, th);
        if let Err(e) = std::fs::write(path, scene::to_ppm(&img)) {
            eprintln!("vista: cannot write {path}: {e}");
            std::process::exit(1);
        }
        return;
    }

    // Animating needs the whole frame on screen; otherwise draw it once.
    let fits = height(l) < rows as usize;

    if args.demo {
        for (label, hr) in [("dawn", 6.25), ("noon", 12.0), ("sunset", 17.75), ("night", 21.5)] {
            println!(" {}{label}\x1b[0m", fg(DIM));
            info.hr = hr;
            play(&info, l, !args.still && fits);
        }
        return;
    }

    let quick_reopen = since_last.is_some_and(|s| s < QUICK_REOPEN_SECS);
    let animate = !args.still && fits && (args.animate || !quick_reopen);
    play(&info, l, animate);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for ch in s.chars() {
            if esc {
                if ch.is_ascii_alphabetic() {
                    esc = false;
                }
            } else if ch == '\x1b' {
                esc = true;
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn info() -> Info {
        Info {
            name: "Aldy".into(),
            hr: 6.7,
            now: sys::LocalTime { month: 10, day: 17, weekday: 6, hour: 6, minute: 42 },
            battery: Some(sys::Battery { percent: 30, charging: true }),
            ram: 64,
            uptime: 140_000,
        }
    }

    #[test]
    fn half_size_sits_beside_the_panel() {
        assert_eq!(layout(120, 40, 50), Layout::Side(48, 30));
        assert_eq!(height(Layout::Side(48, 30)), 15);
    }

    #[test]
    fn narrow_or_short_terminals_fall_back() {
        assert!(matches!(layout(70, 50, 50), Layout::Stack(..)));
        assert!(matches!(layout(120, 14, 50), Layout::Side(w, _) if w < 48));
        assert_eq!(layout(30, 12, 50), Layout::TextOnly);
    }

    #[test]
    fn panel_fits_its_width_and_shows_everything_at_the_end() {
        let lines = panel(END_SECS, &info(), Some(7));
        assert_eq!(lines.len(), PANEL_LINES);
        for l in &lines {
            assert!(strip(l).chars().count() <= PANEL_W, "too wide: {}", strip(l));
        }
        let text: Vec<String> = lines.iter().map(|l| strip(l)).collect();
        assert_eq!(text[0], "Good morning, Aldy");
        assert!(text[3].contains("battery") && text[3].contains("30%"));
        assert!(text[4].contains("cpu") && text[4].contains("7%"));
        assert!(text[7].contains("sunset") && text[7].ends_with("in 11h 18m"));
        assert_eq!(text[11].chars().count(), 18);
    }

    #[test]
    fn side_lines_have_a_fixed_picture_column() {
        let l = layout(120, 40, 50);
        for line in compose(END_SECS, &info(), l, Some(5)) {
            let s = strip(&line);
            assert!(s.chars().count() <= 48 + GAP + PANEL_W);
        }
    }

    #[test]
    fn sun_events_wrap_around_midnight() {
        assert_eq!(sun_event(12.0), ("sunset", 360));
        assert_eq!(sun_event(20.0), ("sunrise", 600));
        assert_eq!(sun_event(3.0), ("sunrise", 180));
        assert_eq!(duration_text(678), "11h 18m");
    }

    #[test]
    fn parses_clock_times() {
        assert_eq!(parse_hhmm("06:30"), Some(6.5));
        assert_eq!(parse_hhmm("25:00"), None);
    }

    #[test]
    fn greets_by_time_of_day() {
        assert_eq!(greeting(7.0, "Aldy"), "Good morning, Aldy");
        assert_eq!(greeting(13.0, "Aldy"), "Good afternoon, Aldy");
        assert_eq!(greeting(19.0, "Aldy"), "Good evening, Aldy");
        assert_eq!(greeting(2.0, "Aldy"), "Still up, Aldy?");
    }
}
