//! The pixel landscape: mountains, pines, a sun or moon that follows the real
//! clock, drifting fog, stars and fireflies.
//!
//! The scene is drawn at 96x60 (ported one-to-one from the browser preview)
//! and then scaled down to the requested size. Scaling keeps small features
//! alive: every pixel carries a priority, and when several pixels merge into
//! one, fireflies, stars, the sun, snow and pines win over sky and rock.

pub const W: usize = 96;
pub const H: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const fn hex(v: u32) -> Rgb {
    Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round().clamp(0.0, 255.0) as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Night,
    Dusk,
    Day,
}

pub fn phase(hr: f32) -> Phase {
    if !(5.5..18.5).contains(&hr) {
        Phase::Night
    } else if (17.0..18.5).contains(&hr) || (5.5..7.0).contains(&hr) {
        Phase::Dusk
    } else {
        Phase::Day
    }
}

/// Colours of the scene for a phase, also used to tint the info panel.
pub struct Palette {
    pub sky_top: Rgb,
    pub sky_bottom: Rgb,
    pub rock: Rgb,
    pub hill: Rgb,
    pub snow: Rgb,
    pub pine: Rgb,
    pub orb: Rgb,
}

pub fn palette(ph: Phase) -> Palette {
    let (sky_top, sky_bottom, rock) = match ph {
        Phase::Night => (hex(0x050816), hex(0x1b1f3a), hex(0x1c2236)),
        Phase::Dusk => (hex(0x2b1b4a), hex(0xf08a5d), hex(0x3b2f55)),
        Phase::Day => (hex(0x3b82c4), hex(0xa5d8f3), hex(0x55708f)),
    };
    Palette {
        sky_top,
        sky_bottom,
        rock,
        hill: if ph == Phase::Night { hex(0x0d131b) } else { hex(0x1e2a36) },
        snow: hex(0xcbd5e1),
        pine: hex(0x14532d),
        orb: if ph == Phase::Night { hex(0xe5e7eb) } else { hex(0xfde68a) },
    }
}

// Pixel priorities used when scaling down
const P_SKY: u8 = 0;
const P_LAND: u8 = 1;
const P_DETAIL: u8 = 2;
const P_LIGHT: u8 = 3;

pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Option<Rgb>>,
    prio: Vec<u8>,
}

impl Img {
    fn new(w: usize, h: usize) -> Img {
        Img { w, h, px: vec![None; w * h], prio: vec![P_SKY; w * h] }
    }

    fn set(&mut self, x: f32, y: f32, c: Rgb, p: u8) {
        let (x, y) = (x.round(), y.round());
        if x >= 0.0 && y >= 0.0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = y as usize * self.w + x as usize;
            self.px[i] = Some(c);
            self.prio[i] = p;
        }
    }

    fn blend_rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: Rgb, alpha: f32) {
        for y in y0.max(0)..(y0 + h).min(self.h as i32) {
            for x in x0.max(0)..(x0 + w).min(self.w as i32) {
                let i = y as usize * self.w + x as usize;
                if let Some(p) = self.px[i] {
                    self.px[i] = Some(mix(p, c, alpha));
                }
            }
        }
    }

    /// Rounded card corners.
    fn round_corners(&mut self, r: f32) {
        let ri = r.ceil() as usize;
        for y in 0..ri.min(self.h) {
            for x in 0..ri.min(self.w) {
                let (dx, dy) = (r - (x as f32 + 0.5), r - (y as f32 + 0.5));
                if dx * dx + dy * dy > r * r {
                    for (cx, cy) in [(x, y), (self.w - 1 - x, y), (x, self.h - 1 - y), (self.w - 1 - x, self.h - 1 - y)]
                    {
                        self.px[cy * self.w + cx] = None;
                    }
                }
            }
        }
    }

    /// Scale down to `tw` x `th`. Each output pixel looks at the block of
    /// source pixels it covers: the most common colour among the highest
    /// priority pixels wins (only details and lights get to override the
    /// plain majority, so ridgelines keep their shape).
    fn downscale(&self, tw: usize, th: usize) -> Img {
        if tw == self.w && th == self.h {
            return Img { w: tw, h: th, px: self.px.clone(), prio: self.prio.clone() };
        }
        let mut out = Img::new(tw, th);
        let mut counts: Vec<(Rgb, u32)> = Vec::with_capacity(16);
        for oy in 0..th {
            let y0 = oy * self.h / th;
            let y1 = ((oy + 1) * self.h / th).max(y0 + 1);
            for ox in 0..tw {
                let x0 = ox * self.w / tw;
                let x1 = ((ox + 1) * self.w / tw).max(x0 + 1);
                let mut top = P_SKY;
                for y in y0..y1 {
                    for x in x0..x1 {
                        top = top.max(self.prio[y * self.w + x]);
                    }
                }
                let wanted = if top >= P_DETAIL { top } else { 0 };
                counts.clear();
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * self.w + x;
                        if self.prio[i] < wanted {
                            continue;
                        }
                        if let Some(c) = self.px[i] {
                            match counts.iter_mut().find(|(k, _)| *k == c) {
                                Some((_, n)) => *n += 1,
                                None => counts.push((c, 1)),
                            }
                        }
                    }
                }
                let best = counts.iter().fold(None::<(Rgb, u32)>, |acc, &(c, n)| match acc {
                    Some((_, m)) if m >= n => acc,
                    _ => Some((c, n)),
                });
                let i = oy * tw + ox;
                out.px[i] = best.map(|(c, _)| c);
                out.prio[i] = top;
            }
        }
        out
    }
}

/// Same LCG as the preview, so the stars land in the same places.
fn stars() -> Vec<(f32, f32, f32)> {
    let mut s: u32 = 42;
    let mut r = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s as f64 / 4_294_967_296.0
    };
    (0..28)
        .map(|_| {
            let x = (r() * W as f64).floor() as f32;
            let y = (r() * 26.0).floor() as f32;
            let p = (r() * 6.0) as f32;
            (x, y, p)
        })
        .collect()
}

fn ridge_back(x: f32) -> f32 {
    52.0 - (24.0 * (-((x - 62.0) / 17.0).powi(2)).exp() + 11.0 * (-((x - 24.0) / 13.0).powi(2)).exp() + 3.0)
}

fn ridge_front(x: f32) -> f32 {
    57.0 - (8.0 * (-((x - 30.0) / 20.0).powi(2)).exp() + 6.0 * (-((x - 82.0) / 14.0).powi(2)).exp())
}

fn pine(img: &mut Img, x: f32, h: i32) {
    for k in 0..h {
        let w = (k as f32 / 2.6).floor() as i32;
        for i in -w..=w {
            let c = if i < 0 { hex(0x0b2e1c) } else { hex(0x14532d) };
            img.set(x + i as f32, (60 - h + k - 2) as f32, c, P_DETAIL);
        }
    }
    img.set(x, 58.0, hex(0x3f2a14), P_DETAIL);
    img.set(x, 59.0, hex(0x3f2a14), P_DETAIL);
}

/// Draw the full 96x60 scene at animation time `t` (seconds) for local hour `hr`.
/// Fog starts thick and clears completely after `fog_secs`.
fn draw(t: f32, hr: f32, fog_secs: f32) -> Img {
    let mut img = Img::new(W, H);
    let ph = phase(hr);
    let pal = palette(ph);
    let (night, dusk) = (ph == Phase::Night, ph == Phase::Dusk);

    for y in 0..H {
        let c = mix(pal.sky_top, pal.sky_bottom, y as f32 / H as f32);
        for x in 0..W {
            img.px[y * W + x] = Some(c);
        }
    }

    if night {
        for (x, y, p) in stars() {
            let s = (t * 2.5 + p).sin();
            if s > -0.2 {
                img.set(x, y, if s > 0.7 { hex(0xffffff) } else { hex(0x94a3b8) }, P_LIGHT);
            }
        }
    }

    for x in 0..W {
        let yb = ridge_back(x as f32).round() as i32;
        for y in yb..H as i32 {
            let snow = y < yb + 2 && yb < 34;
            if snow {
                img.set(x as f32, y as f32, pal.snow, P_DETAIL);
            } else {
                img.set(x as f32, y as f32, pal.rock, P_LAND);
            }
        }
    }
    for x in 0..W {
        let yf = ridge_front(x as f32).round() as i32;
        for y in yf..H as i32 {
            img.set(x as f32, y as f32, pal.hill, P_LAND);
        }
    }

    for (x, h) in [(5, 10), (12, 13), (18, 9), (70, 11), (77, 14), (85, 10), (91, 12)] {
        pine(&mut img, x as f32, h);
    }

    // Fog bands: thick at first, clearing to a faint mist
    let fog = if t < fog_secs { 1.0 - t / fog_secs } else { 0.0 };
    let alpha = 0.75 * fog + 0.06;
    let mut y = 28;
    while y < 58 {
        let dir = if y % 2 == 1 { 1.0 } else { -1.0 };
        let off = (t * 12.0 * dir + y as f32 * 7.0) % W as f32;
        img.blend_rect((off - W as f32).round() as i32, y, (W as f32 * 0.7) as i32, 2, hex(0xe2e8f0), alpha);
        img.blend_rect((off + 8.0).round() as i32, y, (W as f32 * 0.6) as i32, 2, hex(0xe2e8f0), alpha);
        y += 3;
    }

    if night || dusk {
        for i in 0..6 {
            let fi = i as f32;
            let x = 16.0 + fi * 13.0 + (t * 0.7 + fi).sin() * 5.0;
            let y = 50.0 + (t * 1.3 + fi * 2.0).sin() * 3.0;
            if (t * 4.0 + fi * 1.7).sin() > 0.2 {
                img.set(x, y, hex(0xfde047), P_LIGHT);
            }
        }
    }
    img
}

/// Sun by day, moon by night, travelling along an arc with the clock.
/// Returns the centre in 96x60 coordinates.
fn orb_center(hr: f32) -> (f32, f32) {
    let f = if phase(hr) == Phase::Night { ((hr - 18.0) + 24.0) % 24.0 / 12.0 } else { (hr - 6.0) / 12.0 };
    let f = f.clamp(0.0, 1.0);
    (10.0 + f * 76.0, 40.0 - (f * std::f32::consts::PI).sin() * 30.0)
}

/// The scene at `tw` x `th` pixels with rounded corners.
pub fn render(t: f32, hr: f32, fog_secs: f32, tw: usize, th: usize) -> Img {
    let mut img = draw(t, hr, fog_secs).downscale(tw, th);

    // The sun or moon is stamped at the final resolution so it stays round
    // and symmetric. Large sizes centre it on a pixel (an odd disc: exactly the
    // preview's 21 pixels at full size); small sizes centre it on a pixel
    // corner (an even disc, 4x4 with the corners cut at half size).
    let s = tw as f32 / W as f32;
    let (ox, oy) = orb_center(hr);
    let (cx, cy, r) = if s >= 0.75 {
        ((ox.round() + 0.5) * s, (oy.round() + 0.5) * s, 2.3 * s)
    } else {
        ((ox * s).round(), (oy * s).round(), 3.6 * s)
    };
    let orb = palette(phase(hr)).orb;
    for y in 0..img.h {
        for x in 0..img.w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                img.px[y * img.w + x] = Some(orb);
            }
        }
    }

    img.round_corners((3.0 * tw as f32 / W as f32).max(2.0));
    img
}

/// Binary PPM of the picture (transparent corners become black), for checking
/// the art outside a terminal.
pub fn to_ppm(img: &Img) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", img.w, img.h).into_bytes();
    for p in &img.px {
        let c = p.unwrap_or(Rgb(0, 0, 0));
        out.extend_from_slice(&[c.0, c.1, c.2]);
    }
    out
}

pub fn fg(c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}

fn bg(c: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2)
}

/// Terminal lines with half blocks: one cell = two stacked pixels, so every
/// pixel is square. Each line is exactly `img.w` columns wide.
pub fn to_lines(img: &Img) -> Vec<String> {
    let at = |x: usize, y: usize| if y < img.h { img.px[y * img.w + x] } else { None };
    let mut lines = Vec::with_capacity(img.h.div_ceil(2));
    for r in 0..img.h.div_ceil(2) {
        let mut s = String::new();
        let (mut cf, mut cb): (Option<Rgb>, Option<Option<Rgb>>) = (None, None);
        let mut set_fg = |s: &mut String, c: Rgb| {
            if cf != Some(c) {
                s.push_str(&fg(c));
                cf = Some(c);
            }
        };
        let mut set_bg = |s: &mut String, c: Option<Rgb>| {
            if cb != Some(c) {
                match c {
                    Some(c) => s.push_str(&bg(c)),
                    None => s.push_str("\x1b[49m"),
                }
                cb = Some(c);
            }
        };
        for x in 0..img.w {
            match (at(x, 2 * r), at(x, 2 * r + 1)) {
                (Some(t), Some(b)) if t == b => {
                    set_bg(&mut s, None);
                    set_fg(&mut s, t);
                    s.push('█');
                }
                (Some(t), Some(b)) => {
                    set_fg(&mut s, t);
                    set_bg(&mut s, Some(b));
                    s.push('▀');
                }
                (Some(t), None) => {
                    set_bg(&mut s, None);
                    set_fg(&mut s, t);
                    s.push('▀');
                }
                (None, Some(b)) => {
                    set_bg(&mut s, None);
                    set_fg(&mut s, b);
                    s.push('▄');
                }
                (None, None) => {
                    set_bg(&mut s, None);
                    s.push(' ');
                }
            }
        }
        s.push_str("\x1b[0m");
        lines.push(s);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_follow_the_clock() {
        assert_eq!(phase(3.0), Phase::Night);
        assert_eq!(phase(6.0), Phase::Dusk);
        assert_eq!(phase(12.0), Phase::Day);
        assert_eq!(phase(17.5), Phase::Dusk);
        assert_eq!(phase(20.0), Phase::Night);
    }

    #[test]
    fn full_size_render_has_cut_corners() {
        let img = render(10.0, 12.0, 1.5, W, H);
        assert!(img.px[0].is_none());
        assert!(img.px[W - 1].is_none());
        assert!(img.px[30 * W + 40].is_some());
        assert_eq!(to_lines(&img).len(), H / 2);
    }

    #[test]
    fn snow_cap_sits_on_the_peak() {
        let img = render(10.0, 12.0, 1.5, W, H);
        let peak = ridge_back(62.0).round() as usize;
        assert_eq!(img.px[peak * W + 62], Some(hex(0xcbd5e1)));
    }

    #[test]
    fn full_size_sun_matches_the_preview_disc() {
        let img = render(10.0, 12.0, 1.5, W, H);
        let sun = img.px.iter().filter(|p| **p == Some(palette(Phase::Day).orb)).count();
        assert_eq!(sun, 21);
    }

    #[test]
    fn half_size_sun_is_a_symmetric_disc() {
        for hr in [6.7, 9.3, 12.0, 15.55] {
            let img = render(10.0, hr, 1.5, W / 2, H / 2);
            let orb = palette(phase(hr)).orb;
            let pts: Vec<(usize, usize)> = (0..img.h)
                .flat_map(|y| (0..img.w).map(move |x| (x, y)))
                .filter(|&(x, y)| img.px[y * img.w + x] == Some(orb))
                .collect();
            assert_eq!(pts.len(), 12, "hour {hr}");
            let (minx, maxx) = (pts.iter().map(|p| p.0).min().unwrap(), pts.iter().map(|p| p.0).max().unwrap());
            let (miny, maxy) = (pts.iter().map(|p| p.1).min().unwrap(), pts.iter().map(|p| p.1).max().unwrap());
            assert_eq!((maxx - minx, maxy - miny), (3, 3), "hour {hr}");
        }
    }

    #[test]
    fn half_size_keeps_snow_and_fireflies() {
        let pal = palette(Phase::Dusk);
        let img = render(1.65, 6.7, 1.5, W / 2, H / 2);
        assert_eq!((img.w, img.h), (48, 30));
        assert!(img.px.contains(&Some(pal.snow)));
        assert!(img.px.contains(&Some(pal.orb)));
        let fireflies = img.px.iter().filter(|p| **p == Some(hex(0xfde047))).count();
        let full = render(1.65, 6.7, 1.5, W, H).px.iter().filter(|p| **p == Some(hex(0xfde047))).count();
        assert_eq!(fireflies, full);
    }
}
