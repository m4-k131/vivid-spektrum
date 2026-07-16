#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Colormap {
    name: String,
    stops: Vec<(f32, f32, f32, f32)>,
}

impl Colormap {
    pub fn new(name: impl Into<String>, stops: Vec<(f32, f32, f32, f32)>) -> Self {
        Self { name: name.into(), stops }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn build_lut(&self, size: usize) -> Vec<[u8; 3]> {
        let n = size.max(2);
        let mut lut = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / (n - 1) as f32;
            lut.push(self.sample(t));
        }
        lut
    }
    pub fn build_lut_rgba(&self, size: usize) -> Vec<[u8; 4]> {
        let n = size.max(2);
        let mut lut = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / (n - 1) as f32;
            let [r, g, b] = self.sample(t);
            lut.push([r, g, b, 255]);
        }
        lut
    }
    fn sample(&self, t: f32) -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        if t <= self.stops[0].0 {
            return to_rgb(self.stops[0]);
        }
        if t >= self.stops.last().unwrap().0 {
            return to_rgb(*self.stops.last().unwrap());
        }
        for w in self.stops.windows(2) {
            let (p0, r0, g0, b0) = w[0];
            let (p1, r1, g1, b1) = w[1];
            if t >= p0 && t <= p1 {
                let f = if (p1 - p0).abs() > 1e-9 {
                    (t - p0) / (p1 - p0)
                } else {
                    0.0
                };
                return [
                    lerp_u8(r0, r1, f),
                    lerp_u8(g0, g1, f),
                    lerp_u8(b0, b1, f),
                ];
            }
        }
        to_rgb(*self.stops.last().unwrap())
    }
}

fn to_rgb(stop: (f32, f32, f32, f32)) -> [u8; 3] {
    [
        (stop.1.clamp(0.0, 1.0) * 255.0).round() as u8,
        (stop.2.clamp(0.0, 1.0) * 255.0).round() as u8,
        (stop.3.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn lerp_u8(a: f32, b: f32, t: f32) -> u8 {
    ((a + (b - a) * t).clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn builtin_colormap(name: &str) -> Option<Colormap> {
    match name.to_lowercase().as_str() {
        "viridis" => Some(viridis()),
        "inferno" => Some(inferno()),
        "magma" => Some(magma()),
        "plasma" => Some(plasma()),
        "turbo" => Some(turbo()),
        "grayscale" => Some(grayscale()),
        "heat" => Some(heat()),
        "gruvbox-dark" => Some(gruvbox_dark()),
        "gruvbox-dark-5" => Some(gruvbox_dark_5()),
        "catppuccin-mocha" => Some(catppuccin_mocha()),
        "catppuccin-mocha-5" => Some(catppuccin_mocha_5()),
        "nord" => Some(nord()),
        "nord-5" => Some(nord_5()),
        "tokyo-night" => Some(tokyo_night()),
        "tokyo-night-5" => Some(tokyo_night_5()),
        _ => None,
    }
}

pub fn builtin_colormap_names() -> Vec<&'static str> {
    vec!["viridis", "inferno", "magma", "plasma", "turbo", "grayscale", "heat", "gruvbox-dark", "gruvbox-dark-5", "catppuccin-mocha", "catppuccin-mocha-5", "nord", "nord-5", "tokyo-night", "tokyo-night-5"]
}

pub fn default_colormap() -> Colormap {
    viridis()
}

#[derive(Debug, serde::Deserialize)]
struct ColormapFile {
    name: Option<String>,
    stops: Vec<ColormapStop>,
}

#[derive(Debug, serde::Deserialize)]
struct ColormapStop {
    position: f32,
    r: f32,
    g: f32,
    b: f32,
}

pub fn load_colormap_file(path: &std::path::Path) -> Result<Colormap, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read colormap: {e}"))?;
    let file: ColormapFile = toml::from_str(&text).map_err(|e| format!("parse colormap: {e}"))?;
    if file.stops.len() < 2 {
        return Err("colormap needs at least 2 stops".into());
    }
    let name = file.name.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "custom".into())
    });
    let stops: Vec<(f32, f32, f32, f32)> = file
        .stops
        .iter()
        .map(|s| (s.position, s.r, s.g, s.b))
        .collect();
    Ok(Colormap::new(name, stops))
}

pub fn resolve_colormap(name_or_path: &str) -> Result<Colormap, String> {
    if let Some(cm) = builtin_colormap(name_or_path) {
        return Ok(cm);
    }
    let path = std::path::Path::new(name_or_path);
    if path.exists() {
        return load_colormap_file(path);
    }
    Err(format!(
        "unknown colormap '{}'. Available: {:?}, or pass a .toml file path",
        name_or_path,
        builtin_colormap_names()
    ))
}

fn viridis() -> Colormap {
    Colormap {
        name: "viridis".into(),
        stops: vec![
            (0.000, 0.267, 0.004, 0.329),
            (0.125, 0.282, 0.141, 0.458),
            (0.250, 0.229, 0.299, 0.525),
            (0.375, 0.127, 0.449, 0.510),
            (0.500, 0.057, 0.568, 0.417),
            (0.625, 0.170, 0.659, 0.274),
            (0.750, 0.370, 0.731, 0.120),
            (0.875, 0.678, 0.785, 0.052),
            (1.000, 0.993, 0.906, 0.144),
        ],
    }
}

fn inferno() -> Colormap {
    Colormap {
        name: "inferno".into(),
        stops: vec![
            (0.000, 0.001, 0.000, 0.014),
            (0.125, 0.106, 0.038, 0.158),
            (0.250, 0.289, 0.076, 0.219),
            (0.375, 0.492, 0.123, 0.189),
            (0.500, 0.685, 0.207, 0.107),
            (0.625, 0.851, 0.337, 0.045),
            (0.750, 0.962, 0.518, 0.071),
            (0.875, 0.988, 0.733, 0.282),
            (1.000, 0.988, 0.998, 0.645),
        ],
    }
}

fn magma() -> Colormap {
    Colormap {
        name: "magma".into(),
        stops: vec![
            (0.000, 0.001, 0.001, 0.014),
            (0.125, 0.130, 0.052, 0.218),
            (0.250, 0.316, 0.104, 0.334),
            (0.375, 0.512, 0.157, 0.340),
            (0.500, 0.703, 0.222, 0.259),
            (0.625, 0.863, 0.332, 0.163),
            (0.750, 0.964, 0.504, 0.107),
            (0.875, 0.994, 0.718, 0.281),
            (1.000, 0.987, 0.991, 0.650),
        ],
    }
}

fn plasma() -> Colormap {
    Colormap {
        name: "plasma".into(),
        stops: vec![
            (0.000, 0.050, 0.030, 0.528),
            (0.125, 0.291, 0.012, 0.652),
            (0.250, 0.496, 0.012, 0.658),
            (0.375, 0.674, 0.098, 0.559),
            (0.500, 0.822, 0.224, 0.420),
            (0.625, 0.933, 0.374, 0.267),
            (0.750, 0.992, 0.554, 0.127),
            (0.875, 0.987, 0.752, 0.045),
            (1.000, 0.940, 0.975, 0.131),
        ],
    }
}

fn turbo() -> Colormap {
    Colormap {
        name: "turbo".into(),
        stops: vec![
            (0.000, 0.190, 0.072, 0.232),
            (0.125, 0.152, 0.330, 0.844),
            (0.250, 0.135, 0.577, 0.993),
            (0.375, 0.304, 0.775, 0.762),
            (0.500, 0.560, 0.886, 0.376),
            (0.625, 0.793, 0.901, 0.106),
            (0.750, 0.940, 0.735, 0.031),
            (0.875, 0.940, 0.439, 0.028),
            (1.000, 0.900, 0.125, 0.055),
        ],
    }
}

fn grayscale() -> Colormap {
    Colormap {
        name: "grayscale".into(),
        stops: vec![
            (0.0, 0.0, 0.0, 0.0),
            (1.0, 1.0, 1.0, 1.0),
        ],
    }
}

fn heat() -> Colormap {
    Colormap {
        name: "heat".into(),
        stops: vec![
            (0.0, 0.0, 0.0, 0.0),
            (0.25, 0.5, 0.0, 0.0),
            (0.50, 1.0, 0.5, 0.0),
            (0.75, 1.0, 1.0, 0.5),
            (1.0, 1.0, 1.0, 1.0),
        ],
    }
}

fn gruvbox_dark() -> Colormap {
    Colormap {
        name: "gruvbox-dark".into(),
        stops: vec![
            (0.000, 0.157, 0.157, 0.157),
            (0.125, 0.251, 0.165, 0.118),
            (0.250, 0.420, 0.220, 0.055),
            (0.375, 0.616, 0.000, 0.024),
            (0.500, 0.800, 0.149, 0.020),
            (0.625, 0.839, 0.380, 0.055),
            (0.750, 0.847, 0.600, 0.129),
            (0.875, 0.922, 0.859, 0.698),
            (1.000, 0.984, 0.945, 0.867),
        ],
    }
}

fn gruvbox_dark_5() -> Colormap {
    Colormap {
        name: "gruvbox-dark-5".into(),
        stops: vec![
            (0.0, 0.114, 0.125, 0.129),
            (0.25, 0.224, 0.180, 0.118),
            (0.5, 0.459, 0.196, 0.078),
            (0.75, 0.682, 0.455, 0.169),
            (1.0, 0.878, 0.776, 0.569),
        ],
    }
}

fn catppuccin_mocha() -> Colormap {
    Colormap {
        name: "catppuccin-mocha".into(),
        stops: vec![
            (0.000, 0.118, 0.125, 0.141),
            (0.125, 0.267, 0.153, 0.314),
            (0.250, 0.490, 0.227, 0.545),
            (0.375, 0.729, 0.333, 0.612),
            (0.500, 0.878, 0.475, 0.565),
            (0.625, 0.945, 0.635, 0.455),
            (0.750, 0.976, 0.788, 0.380),
            (0.875, 0.980, 0.890, 0.486),
            (1.000, 0.906, 0.890, 0.765),
        ],
    }
}

fn catppuccin_mocha_5() -> Colormap {
    Colormap {
        name: "catppuccin-mocha-5".into(),
        stops: vec![
            (0.0, 0.118, 0.125, 0.141),
            (0.25, 0.490, 0.227, 0.545),
            (0.5, 0.878, 0.475, 0.565),
            (0.75, 0.976, 0.788, 0.380),
            (1.0, 0.906, 0.890, 0.765),
        ],
    }
}

fn nord() -> Colormap {
    Colormap {
        name: "nord".into(),
        stops: vec![
            (0.000, 0.180, 0.204, 0.251),
            (0.125, 0.263, 0.314, 0.376),
            (0.250, 0.369, 0.424, 0.502),
            (0.375, 0.529, 0.553, 0.627),
            (0.500, 0.698, 0.667, 0.749),
            (0.625, 0.812, 0.733, 0.804),
            (0.750, 0.878, 0.808, 0.859),
            (0.875, 0.925, 0.882, 0.910),
            (1.000, 0.925, 0.910, 0.925),
        ],
    }
}

fn nord_5() -> Colormap {
    Colormap {
        name: "nord-5".into(),
        stops: vec![
            (0.0, 0.180, 0.204, 0.251),
            (0.25, 0.369, 0.424, 0.502),
            (0.5, 0.698, 0.667, 0.749),
            (0.75, 0.878, 0.808, 0.859),
            (1.0, 0.925, 0.910, 0.925),
        ],
    }
}

fn tokyo_night() -> Colormap {
    Colormap {
        name: "tokyo-night".into(),
        stops: vec![
            (0.000, 0.102, 0.110, 0.180),
            (0.125, 0.149, 0.165, 0.290),
            (0.250, 0.247, 0.286, 0.490),
            (0.375, 0.384, 0.420, 0.643),
            (0.500, 0.545, 0.553, 0.753),
            (0.625, 0.698, 0.667, 0.816),
            (0.750, 0.816, 0.733, 0.851),
            (0.875, 0.898, 0.820, 0.890),
            (1.000, 0.757, 0.875, 0.980),
        ],
    }
}

fn tokyo_night_5() -> Colormap {
    Colormap {
        name: "tokyo-night-5".into(),
        stops: vec![
            (0.0, 0.102, 0.110, 0.180),
            (0.25, 0.247, 0.286, 0.490),
            (0.5, 0.545, 0.553, 0.753),
            (0.75, 0.816, 0.733, 0.851),
            (1.0, 0.757, 0.875, 0.980),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_size_256() {
        let cmap = viridis();
        let lut = cmap.build_lut(256);
        assert_eq!(lut.len(), 256);
    }

    #[test]
    fn lut_minimum_size_two() {
        let cmap = viridis();
        let lut = cmap.build_lut(1);
        assert_eq!(lut.len(), 2);
    }

    #[test]
    fn lut_first_is_dark() {
        let cmap = viridis();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[0];
        assert!(r < 80 && g < 10 && b < 100, "viridis start should be dark purple");
    }

    #[test]
    fn lut_last_is_bright() {
        let cmap = viridis();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[255];
        assert!(r > 200 && g > 200 && b < 100, "viridis end should be bright yellow");
    }

    #[test]
    fn grayscale_lut_is_neutral() {
        let cmap = grayscale();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[128];
        assert!((r as i32 - g as i32).abs() <= 1);
        assert!((g as i32 - b as i32).abs() <= 1);
    }

    #[test]
    fn heat_lut_starts_black() {
        let cmap = heat();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[0];
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn heat_lut_ends_white() {
        let cmap = heat();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[255];
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
    }

    #[test]
    fn all_builtin_colormaps_exist() {
        for name in builtin_colormap_names() {
            let cmap = builtin_colormap(name);
            assert!(cmap.is_some(), "colormap '{name}' should exist");
            let lut = cmap.unwrap().build_lut(256);
            assert_eq!(lut.len(), 256);
        }
    }

    #[test]
    fn unknown_colormap_returns_none() {
        assert!(builtin_colormap("nonexistent").is_none());
    }

    #[test]
    fn default_colormap_is_viridis() {
        let cmap = default_colormap();
        assert_eq!(cmap.name(), "viridis");
    }

    #[test]
    fn sample_at_zero_matches_first_stop() {
        let cmap = viridis();
        let lut = cmap.build_lut(256);
        let c0 = cmap.sample(0.0);
        assert_eq!(c0, lut[0]);
    }

    #[test]
    fn sample_at_one_matches_last_stop() {
        let cmap = viridis();
        let lut = cmap.build_lut(256);
        let c1 = cmap.sample(1.0);
        assert_eq!(c1, lut[255]);
    }

    #[test]
    fn sample_clamps_out_of_range() {
        let cmap = viridis();
        let below = cmap.sample(-0.5);
        let above = cmap.sample(1.5);
        let at_zero = cmap.sample(0.0);
        let at_one = cmap.sample(1.0);
        assert_eq!(below, at_zero);
        assert_eq!(above, at_one);
    }

    #[test]
    fn to_rgb_scales_correctly() {
        assert_eq!(to_rgb((0.0, 0.0, 0.0, 0.0)), [0, 0, 0]);
        assert_eq!(to_rgb((0.0, 1.0, 1.0, 1.0)), [255, 255, 255]);
        assert_eq!(to_rgb((0.0, 0.5, 0.0, 0.0)), [128, 0, 0]);
    }

    #[test]
    fn lerp_u8_interpolates() {
        assert_eq!(lerp_u8(0.0, 1.0, 0.0), 0);
        assert_eq!(lerp_u8(0.0, 1.0, 1.0), 255);
        assert_eq!(lerp_u8(0.0, 1.0, 0.5), 128);
    }

    #[test]
    fn lut_is_monotonically_increasing_in_brightness() {
        let cmap = grayscale();
        let lut = cmap.build_lut(256);
        for i in 1..256 {
            assert!(lut[i][0] >= lut[i - 1][0]);
        }
    }
}
