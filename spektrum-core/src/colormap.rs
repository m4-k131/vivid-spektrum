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
    pub fn stops(&self) -> &[(f32, f32, f32, f32)] {
        &self.stops
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

pub fn colormaps_dir() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or(std::path::Path::new("."));
    for candidate in [
        std::path::PathBuf::from("colormaps"),
        std::path::PathBuf::from("../colormaps"),
        exe_dir.join("colormaps"),
        exe_dir.join("../colormaps"),
        exe_dir.join("../../colormaps"),
        exe_dir.join("../../../colormaps"),
    ] {
        if candidate.is_dir() {
            return candidate;
        }
    }
    std::path::PathBuf::from("colormaps")
}

pub fn user_colormaps_dir() -> std::path::PathBuf {
    config_dir().join("vividspektrum/colormaps")
}

pub fn is_builtin_colormap(name: &str) -> bool {
    colormaps_dir().join(format!("{}.toml", name.to_lowercase())).exists()
}

pub fn builtin_colormap(name: &str) -> Option<Colormap> {
    let dir = colormaps_dir();
    let path = dir.join(format!("{}.toml", name.to_lowercase()));
    if path.exists() {
        return load_colormap_file(&path).ok();
    }
    None
}

pub fn builtin_colormap_names() -> Vec<String> {
    let dir = colormaps_dir();
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().into_owned());
                }
            }
        }
    }
    names.sort();
    names
}

pub fn save_user_colormap(name: &str, colormap: &Colormap) -> Result<(), String> {
    validate_name(name)?;
    if is_builtin_colormap(name) {
        return Err(format!("'{name}' is a protected built-in colormap"));
    }
    if colormap.stops.len() < 2 || colormap.stops.iter().any(|(p, r, g, b)| !(0.0..=1.0).contains(p) || !(0.0..=1.0).contains(r) || !(0.0..=1.0).contains(g) || !(0.0..=1.0).contains(b)) {
        return Err("colormap stops must contain values from 0 to 1".to_string());
    }
    let mut stops = colormap.stops.clone();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let dir = user_colormaps_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create colormap directory: {e}"))?;
    let path = dir.join(format!("{name}.toml"));
    let file = ColormapFile { name: Some(name.to_string()), stops: stops.into_iter().map(|(position, r, g, b)| ColormapStop { position, r, g, b }).collect() };
    let text = toml::to_string_pretty(&file).map_err(|e| format!("serialize colormap: {e}"))?;
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).map_err(|e| format!("write colormap: {e}"))?;
    std::fs::rename(temp, path).map_err(|e| format!("save colormap: {e}"))
}

pub fn delete_user_colormap(name: &str) -> Result<(), String> {
    validate_name(name)?;
    std::fs::remove_file(user_colormaps_dir().join(format!("{name}.toml"))).map_err(|e| format!("delete colormap: {e}"))
}

pub fn default_colormap() -> Colormap {
    builtin_colormap("viridis").unwrap_or_else(|| Colormap {
        name: "fallback-grayscale".into(),
        stops: vec![(0.0, 0.0, 0.0, 0.0), (1.0, 1.0, 1.0, 1.0)],
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ColormapFile {
    name: Option<String>,
    stops: Vec<ColormapStop>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
    let user_path = user_colormaps_dir().join(format!("{name_or_path}.toml"));
    if user_path.exists() {
        return load_colormap_file(&user_path);
    }
    let path = std::path::Path::new(name_or_path);
    if path.exists() {
        return load_colormap_file(path);
    }
    Err(format!(
        "unknown colormap '{}'. Available: {:?}, or pass a .toml file path",
        name_or_path,
        all_colormap_names()
    ))
}

pub fn all_colormap_names() -> Vec<String> {
    let mut names = builtin_colormap_names();
    if let Ok(entries) = std::fs::read_dir(user_colormaps_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                    if !names.contains(&name) { names.push(name); }
                }
            }
        }
    }
    names.sort();
    names
}

fn config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("names may contain only letters, numbers, '-' and '_'".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_size_256() {
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(256);
        assert_eq!(lut.len(), 256);
    }

    #[test]
    fn lut_minimum_size_two() {
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(1);
        assert_eq!(lut.len(), 2);
    }

    #[test]
    fn lut_first_is_dark() {
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[0];
        assert!(r < 80 && g < 10 && b < 100, "viridis start should be dark purple");
    }

    #[test]
    fn lut_last_is_bright() {
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[255];
        assert!(r > 200 && g > 200 && b < 100, "viridis end should be bright yellow");
    }

    #[test]
    fn grayscale_lut_is_neutral() {
        let cmap = builtin_colormap("grayscale").unwrap();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[128];
        assert!((r as i32 - g as i32).abs() <= 1);
        assert!((g as i32 - b as i32).abs() <= 1);
    }

    #[test]
    fn heat_lut_starts_black() {
        let cmap = builtin_colormap("heat").unwrap();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[0];
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn heat_lut_ends_white() {
        let cmap = builtin_colormap("heat").unwrap();
        let lut = cmap.build_lut(256);
        let [r, g, b] = lut[255];
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
    }

    #[test]
    fn all_builtin_colormaps_exist() {
        let names = builtin_colormap_names();
        assert!(!names.is_empty(), "colormaps/ directory should have .toml files");
        for name in &names {
            let cmap = builtin_colormap(name);
            assert!(cmap.is_some(), "colormap '{name}' should load");
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
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(256);
        let c0 = cmap.sample(0.0);
        assert_eq!(c0, lut[0]);
    }

    #[test]
    fn sample_at_one_matches_last_stop() {
        let cmap = builtin_colormap("viridis").unwrap();
        let lut = cmap.build_lut(256);
        let c1 = cmap.sample(1.0);
        assert_eq!(c1, lut[255]);
    }

    #[test]
    fn sample_clamps_out_of_range() {
        let cmap = builtin_colormap("viridis").unwrap();
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
        let cmap = builtin_colormap("grayscale").unwrap();
        let lut = cmap.build_lut(256);
        for i in 1..256 {
            assert!(lut[i][0] >= lut[i - 1][0]);
        }
    }
}
