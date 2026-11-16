use crate::abi::{BAD_REQUEST, MAX_BYTES};
use crate::baritone_utils::Pos;
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, i32>;

pub fn parse(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > MAX_BYTES {
        return Err(BAD_REQUEST);
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| BAD_REQUEST)?;
    if !v.is_object() || v["schema"].as_u64() != Some(1) {
        return Err(BAD_REQUEST);
    }
    Ok(v)
}

pub fn id(v: &Value, key: &str) -> Result<u64> {
    let text = v[key].as_str().ok_or(BAD_REQUEST)?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BAD_REQUEST);
    }
    text.parse().map_err(|_| BAD_REQUEST)
}

pub fn integer(v: &Value) -> Result<i32> {
    i32::try_from(v.as_i64().ok_or(BAD_REQUEST)?).map_err(|_| BAD_REQUEST)
}

pub fn pos(v: &Value) -> Result<Pos> {
    let a = v.as_array().filter(|a| a.len() == 3).ok_or(BAD_REQUEST)?;
    let p = [integer(&a[0])?, integer(&a[1])?, integer(&a[2])?];
    if p.iter().any(|n| !(-30_000_000..=30_000_000).contains(n)) {
        return Err(BAD_REQUEST);
    }
    Ok(p)
}

pub fn number(v: &Value) -> Result<f64> {
    v.as_f64().filter(|n| n.is_finite()).ok_or(BAD_REQUEST)
}

pub fn vec3(v: &Value) -> Result<[f64; 3]> {
    let a = v.as_array().filter(|a| a.len() == 3).ok_or(BAD_REQUEST)?;
    let p = [number(&a[0])?, number(&a[1])?, number(&a[2])?];
    if p.iter().any(|n| n.abs() > 30_000_000.0) {
        return Err(BAD_REQUEST);
    }
    Ok(p)
}

pub fn dimension(v: &Value) -> Result<String> {
    let text = v
        .as_str()
        .filter(|s| {
            s.len() <= 256
                && s.contains(':')
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:./-".contains(&b))
        })
        .ok_or(BAD_REQUEST)?;
    Ok(text.to_owned())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub world: u64,
    pub dimension: String,
    pub job: u64,
    pub lease: u64,
}

impl Identity {
    pub fn read(v: &Value) -> Result<Self> {
        let value = Self {
            world: id(v, "world")?,
            dimension: dimension(&v["dimension"])?,
            job: id(v, "job_generation")?,
            lease: id(v, "lease_generation")?,
        };
        if value.world == 0 || value.job == 0 || value.lease == 0 {
            return Err(BAD_REQUEST);
        }
        Ok(value)
    }
    pub fn write(&self, v: &mut Value) {
        v["world"] = json!(self.world.to_string());
        v["dimension"] = json!(self.dimension);
        v["job_generation"] = json!(self.job.to_string());
        v["lease_generation"] = json!(self.lease.to_string());
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub identity: Identity,
    pub tick: u64,
    pub registry: u64,
    pub revision: u64,
    pub settings_revision: u64,
    pub target_revision: u64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub input_available: bool,
    pub selected_slot: usize,
    pub min_y: i32,
    pub max_y: i32,
    pub raw: Value,
}

impl Frame {
    pub fn read(v: &Value) -> Result<Self> {
        let player = &v["player"];
        let position = vec3(&player["position"])?;
        let yaw = number(&player["yaw"])?;
        let pitch = number(&player["pitch"])?;
        if yaw.abs() > 1.0e9 || !(-90.0..=90.0).contains(&pitch) {
            return Err(BAD_REQUEST);
        }
        let velocity = if player["velocity"].is_null() {
            [0.0; 3]
        } else {
            vec3(&player["velocity"])?
        };
        if velocity.iter().any(|n| n.abs() > 128.0) {
            return Err(BAD_REQUEST);
        }
        let selected_slot = v["selected_slot"].as_u64().unwrap_or(0) as usize;
        if selected_slot >= 9 {
            return Err(BAD_REQUEST);
        }
        let min_y = v["min_y"].as_i64().unwrap_or(-64);
        let max_y = v["max_y"].as_i64().unwrap_or(319);
        if !(-4096..=4095).contains(&min_y) || !(-4096..=4095).contains(&max_y) || min_y >= max_y {
            return Err(BAD_REQUEST);
        }
        for key in ["eye_height", "water_height", "movement_speed", "reach"] {
            if !player[key].is_null() {
                number(&player[key])?;
            }
        }
        Ok(Self {
            identity: Identity::read(v)?,
            tick: id(v, "tick")?,
            registry: id(v, "registry_epoch")?,
            revision: id(v, "block_revision")?,
            settings_revision: id(v, "settings_revision")?,
            target_revision: id(v, "target_revision")?,
            position,
            velocity,
            yaw: yaw as f32,
            pitch: pitch as f32,
            input_available: v["input_available"].as_bool().ok_or(BAD_REQUEST)?,
            selected_slot,
            min_y: min_y as i32,
            max_y: max_y as i32,
            raw: v.clone(),
        })
    }
    pub fn flag(&self, key: &str, fallback: bool) -> bool {
        self.raw["player"][key].as_bool().unwrap_or(fallback)
    }
    pub fn fact(&self, key: &str, fallback: f64) -> f64 {
        self.raw["player"][key].as_f64().unwrap_or(fallback)
    }
    pub fn feet(&self) -> Pos {
        self.position.map(|n| n.floor() as i32)
    }
    pub fn eye(&self) -> [f64; 3] {
        [
            self.position[0],
            self.position[1] + self.fact("eye_height", 1.62).clamp(0.2, 2.0),
            self.position[2],
        ]
    }
    pub fn reach(&self) -> f64 {
        self.fact("reach", 4.5).clamp(0.0, 6.0)
    }
}
