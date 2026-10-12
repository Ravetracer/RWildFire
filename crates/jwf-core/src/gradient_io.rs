//! Gradient files (UltraFractal `.ugr`, Fractint `.map`, flam3 palette XML) and key-point fitting.

use std::fmt::Write as _;

use crate::gradient::{GRADIENT_SIZE, Gradient};

/// A named gradient from a file or library.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedGradient {
    pub name: String,
    pub gradient: Gradient,
}

/// Parses all gradients of an UltraFractal `.ugr` file (positions 0..399).
pub fn parse_ugr(src: &str) -> Vec<NamedGradient> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(ps) = src[pos..].find('{').map(|i| i + pos) {
        let Some(pe) = src[ps + 1..].find('}').map(|i| i + ps + 1) else { break };
        pos = pe + 1;
        let block = &src[ps..pe];
        if !(block.contains("title=\"") && block.contains("color=") && block.contains("index=")) {
            continue;
        }
        let name = block
            .find("title=\"")
            .and_then(|i| block[i + 7..].find('"').map(|j| block[i + 7..i + 7 + j].to_string()))
            .unwrap_or_default();
        let mut keys: Vec<(usize, [u8; 3])> = Vec::new();
        let mut tokens = block.split_whitespace().peekable();
        while let Some(t) = tokens.next() {
            if let Some(idx) = t.strip_prefix("index=")
                && let Some(next) = tokens.peek()
                && let Some(col) = next.strip_prefix("color=")
            {
                if let (Ok(i), Ok(c)) = (idx.parse::<i64>(), col.parse::<i64>()) {
                    let c = c as u32;
                    keys.push((i.clamp(0, 399) as usize, [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8]));
                }
                tokens.next();
            }
        }
        if keys.is_empty() {
            continue;
        }
        keys.sort_by_key(|k| k.0);
        let gradient = if keys.len() >= GRADIENT_SIZE {
            // Dense definition: step function over 0..399, sampled at 256 positions.
            let mut dense = vec![[0u8; 3]; 400];
            let mut last = [0u8; 3];
            let mut ki = 0;
            for (i, slot) in dense.iter_mut().enumerate() {
                while ki < keys.len() && keys[ki].0 <= i {
                    last = keys[ki].1;
                    ki += 1;
                }
                *slot = last;
            }
            Gradient::from_colors((0..GRADIENT_SIZE).map(|i| dense[i * 399 / (GRADIENT_SIZE - 1)]).collect())
        } else {
            let mut k: Vec<(f64, [u8; 3])> = keys.iter().map(|(i, c)| (*i as f64 / 399.0, *c)).collect();
            if k.first().is_some_and(|f| f.0 > 0.0) {
                k.insert(0, (0.0, [0, 0, 0]));
            }
            if k.last().is_some_and(|l| l.0 < 1.0) {
                k.push((1.0, [0, 0, 0]));
            }
            Gradient::from_keys(&k)
        };
        out.push(NamedGradient { name, gradient });
    }
    out
}

/// Writes gradients as UltraFractal `.ugr` (all 256 colors as keys).
pub fn write_ugr(gradients: &[NamedGradient]) -> String {
    let mut s = String::new();
    for (n, g) in gradients.iter().enumerate() {
        let title = if g.name.is_empty() { format!("gradient{n}") } else { g.name.replace('"', "'") };
        let ident: String = title.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let _ = writeln!(s, "{ident} {{\ngradient:\n title=\"{title}\" smooth=no");
        for (i, c) in g.gradient.colors.iter().enumerate() {
            let idx = i * 399 / (GRADIENT_SIZE - 1);
            let col = c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16;
            let _ = writeln!(s, " index={idx} color={col}");
        }
        s.push_str("}\n\n");
    }
    s
}

/// Parses a Fractint `.map` file (one "r g b" triple per line).
pub fn parse_map(src: &str, name: &str) -> Option<NamedGradient> {
    let mut colors = Vec::new();
    for line in src.lines() {
        let mut it = line.split_whitespace().map(|t| t.parse::<i64>());
        match (it.next(), it.next(), it.next()) {
            (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) if (0..=255).contains(&r) && (0..=255).contains(&g) && (0..=255).contains(&b) => {
                colors.push([r as u8, g as u8, b as u8]);
            }
            _ => break,
        }
    }
    (!colors.is_empty()).then(|| NamedGradient { name: name.to_string(), gradient: Gradient::from_colors(colors) })
}

/// Parses flam3's `flam3-palettes.xml` (`<palette name=".." data="00RRGGBB...">`).
pub fn parse_flam3_palettes(src: &str) -> Vec<NamedGradient> {
    let Ok(root) = crate::io::xml::parse(src) else { return Vec::new() };
    let mut list = Vec::new();
    root.find_all("palette", &mut list);
    list.into_iter()
        .filter_map(|p| {
            let data: String = p.attr("data").unwrap_or(&p.text).chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if data.len() < 8 {
                return None;
            }
            let colors: Vec<[u8; 3]> = (0..data.len() / 8)
                .map(|i| {
                    let h = |o: usize| u8::from_str_radix(&data[i * 8 + o..i * 8 + o + 2], 16).unwrap_or(0);
                    [h(2), h(4), h(6)]
                })
                .collect();
            let name = p.attr("name").unwrap_or("").to_string();
            Some(NamedGradient { name, gradient: Gradient::from_colors(colors) })
        })
        .collect()
}

/// Detects the format from the file name / content.
pub fn parse_any(src: &str, file_name: &str) -> Vec<NamedGradient> {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".map") {
        let stem = std::path::Path::new(file_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        return parse_map(src, &stem).into_iter().collect();
    }
    if src.contains("<palette") {
        return parse_flam3_palettes(src);
    }
    parse_ugr(src)
}

/// A gradient key point: position 0..1 and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub pos: f64,
    pub color: [u8; 3],
}

/// Approximates a 256-color gradient by a small set of stops with linear interpolation
/// (greedy refinement until the max channel error is below `tolerance` or `max_stops` is reached).
pub fn fit_stops(g: &Gradient, tolerance: f64, max_stops: usize) -> Vec<Stop> {
    let n = g.colors.len();
    if n < 2 {
        return vec![Stop { pos: 0.0, color: g.colors.first().copied().unwrap_or([0; 3]) }];
    }
    let mut idx: Vec<usize> = vec![0, n - 1];
    loop {
        let mut worst = (0.0f64, 0usize);
        for w in idx.windows(2) {
            let (a, b) = (w[0], w[1]);
            for i in a + 1..b {
                let t = (i - a) as f64 / (b - a) as f64;
                let err = (0..3)
                    .map(|c| {
                        let interp = g.colors[a][c] as f64 + (g.colors[b][c] as f64 - g.colors[a][c] as f64) * t;
                        (interp - g.colors[i][c] as f64).abs()
                    })
                    .fold(0.0, f64::max);
                if err > worst.0 {
                    worst = (err, i);
                }
            }
        }
        if worst.0 <= tolerance || idx.len() >= max_stops {
            break;
        }
        let pos = idx.partition_point(|&x| x < worst.1);
        idx.insert(pos, worst.1);
    }
    idx.into_iter().map(|i| Stop { pos: i as f64 / (n - 1) as f64, color: g.colors[i] }).collect()
}

/// Renders stops into a 256-color gradient.
pub fn stops_to_gradient(stops: &[Stop]) -> Gradient {
    if stops.is_empty() {
        return Gradient::default();
    }
    let keys: Vec<(f64, [u8; 3])> = stops.iter().map(|s| (s.pos.clamp(0.0, 1.0), s.color)).collect();
    Gradient::from_keys(&keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ugr_roundtrip() {
        let mut rng = crate::rng::Rng::new(5);
        let g = Gradient::random(&mut rng);
        let txt = write_ugr(&[NamedGradient { name: "test".into(), gradient: g.clone() }]);
        let back = parse_ugr(&txt);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "test");
        assert_eq!(back[0].gradient, g);
    }

    #[test]
    fn sparse_ugr_interpolates() {
        let src = "a {\ngradient:\n title=\"two\" smooth=no\n index=0 color=255\n index=399 color=16711680\n}\n";
        let g = &parse_ugr(src)[0].gradient;
        assert_eq!(g.colors[0], [255, 0, 0]);
        assert_eq!(g.colors[255], [0, 0, 255]);
    }

    #[test]
    fn fit_reproduces_gradient() {
        let mut rng = crate::rng::Rng::new(11);
        let g = Gradient::random(&mut rng);
        let stops = fit_stops(&g, 3.0, 64);
        let back = stops_to_gradient(&stops);
        for (a, b) in g.colors.iter().zip(&back.colors) {
            for c in 0..3 {
                assert!((a[c] as i32 - b[c] as i32).abs() <= 4);
            }
        }
        assert!(stops.len() < 40, "too many stops: {}", stops.len());
    }

    #[test]
    fn map_parses() {
        let g = parse_map("0 0 0\n255 255 255\n", "bw").unwrap();
        assert_eq!(g.gradient.colors[0], [0, 0, 0]);
        assert_eq!(g.gradient.colors[255], [255, 255, 255]);
    }
}
