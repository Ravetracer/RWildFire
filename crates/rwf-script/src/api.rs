// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! The Lua API: the `rwf` module and handles to flames, layers, transforms and variations.
//!
//! Handles keep a shared reference to their flame plus indices, so `flame:xform(1).weight = 2`
//! changes the flame in place. A handle whose transform was removed reports an error on use.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, UserDataRef, Value, Variadic};
use rwf_core::animation::{all_props, get_prop, set_prop};
use rwf_core::gradient::Gradient;
use rwf_core::model::{Affine, DrawMode, Flame, Layer, VariationInstance, XForm, XFormKind};
use rwf_core::random::extras::{Extras, SymmetryGen, WeightingGen};
use rwf_core::random::gradients::{GradientGenerator, random_gradient_of};
use rwf_core::random::mutations::{MutationType, mutate_flame};
use rwf_core::random::{Generator, generate_with};
use rwf_core::rng::Rng;
use rwf_core::variations;

use crate::ParamValue;

/// Reference of the Lua API (help window, `rwf-cli script --api`).
pub const API_REFERENCE: &str = include_str!("../API.md");

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(mlua::Error::runtime(msg.into()))
}

// ---------------------------------------------------------------------------------------------
// Run state

/// Receiver of `print` lines.
pub type PrintFn = Box<dyn Fn(&str)>;

struct Inner {
    base_dir: Option<PathBuf>,
    on_print: Option<PrintFn>,
    log: Vec<String>,
    outputs: Vec<Flame>,
    rng: Rng,
}

/// State shared by the API functions of one run.
#[derive(Clone)]
pub struct State(Rc<RefCell<Inner>>);

impl State {
    pub fn new(base_dir: Option<PathBuf>, on_print: Option<PrintFn>) -> Self {
        State(Rc::new(RefCell::new(Inner { base_dir, on_print, log: Vec::new(), outputs: Vec::new(), rng: Rng::from_entropy() })))
    }

    pub fn log(&self) -> Vec<String> {
        self.0.borrow().log.clone()
    }

    pub fn take_outputs(&self) -> Vec<Flame> {
        std::mem::take(&mut self.0.borrow_mut().outputs)
    }

    fn print(&self, line: String) {
        let mut s = self.0.borrow_mut();
        if let Some(f) = &s.on_print {
            f(&line);
        }
        s.log.push(line);
    }

    fn path(&self, p: &str) -> PathBuf {
        let p = Path::new(p);
        match &self.0.borrow().base_dir {
            Some(dir) if p.is_relative() => dir.join(p),
            _ => p.to_path_buf(),
        }
    }

    fn with_rng<R>(&self, f: impl FnOnce(&mut Rng) -> R) -> R {
        f(&mut self.0.borrow_mut().rng)
    }
}

// ---------------------------------------------------------------------------------------------
// Value helpers

fn num(v: &Value, what: &str) -> Result<f64> {
    match v {
        Value::Integer(i) => Ok(*i as f64),
        Value::Number(n) => Ok(*n),
        Value::String(s) => s.to_str().ok().and_then(|s| s.trim().parse().ok()).map_or_else(|| err(format!("{what} must be a number")), Ok),
        other => err(format!("{what} must be a number, not a {}", other.type_name())),
    }
}

fn boolean(v: &Value, what: &str) -> Result<bool> {
    match v {
        Value::Boolean(b) => Ok(*b),
        other => err(format!("{what} must be true or false, not a {}", other.type_name())),
    }
}

fn text(v: &Value, what: &str) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(_) | Value::Number(_) => Ok(num(v, what)?.to_string()),
        other => err(format!("{what} must be a string, not a {}", other.type_name())),
    }
}

/// 1-based Lua index -> 0-based index below `len`.
fn index(i: i64, len: usize, what: &str) -> Result<usize> {
    if i >= 1 && (i as usize) <= len {
        Ok(i as usize - 1)
    } else if len == 0 {
        err(format!("{what} {i} does not exist (there are none)"))
    } else {
        err(format!("{what} {i} does not exist (1 to {len})"))
    }
}

fn channel(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn find_by_label<T: Copy>(all: &[T], label: impl Fn(T) -> &'static str, name: &str, what: &str) -> Result<T> {
    // "All" finds "(All)".
    let plain = |s: &str| s.trim_matches(|c| c == '(' || c == ')').to_ascii_lowercase();
    if let Some(t) = all.iter().copied().find(|t| plain(label(*t)) == plain(name)) {
        return Ok(t);
    }
    let names: Vec<&str> = all.iter().map(|t| label(*t)).collect();
    err(format!("unknown {what} '{name}' (one of: {})", names.join(", ")))
}

// ---------------------------------------------------------------------------------------------
// Flame

/// A flame (shared, changed in place by all its handles).
#[derive(Clone)]
pub struct LFlame(Rc<RefCell<Flame>>);

impl LFlame {
    fn new(f: Flame) -> Self {
        LFlame(Rc::new(RefCell::new(f)))
    }

    pub fn snapshot(&self) -> Flame {
        self.0.borrow().clone()
    }

    fn layer(&self, i: i64) -> Result<LLayer> {
        let n = self.0.borrow().layers.len();
        Ok(LLayer { f: self.0.clone(), l: index(i, n, "layer")? })
    }

    fn first(&self) -> LLayer {
        LLayer { f: self.0.clone(), l: 0 }
    }
}

/// Plain numeric flame settings that are not animatable (`get_prop` covers the others).
fn flame_number(f: &Flame, key: &str) -> Option<f64> {
    Some(match key {
        "width" => f.width as f64,
        "height" => f.height as f64,
        "sample_density" => f.sample_density,
        "spatial_filter_radius" => f.spatial_filter_radius,
        "spatial_oversampling" => f.spatial_oversampling as f64,
        "filter_sharpness" => f.filter_sharpness,
        "filter_low_density" => f.filter_low_density,
        "post_blur_radius" => f.post_blur_radius as f64,
        "post_blur_fade" => f.post_blur_fade,
        "post_blur_falloff" => f.post_blur_falloff,
        "de_radius" => f.de_radius,
        "de_curve" => f.de_curve,
        "antialias_amount" => f.antialias_amount,
        "antialias_radius" => f.antialias_radius,
        "post_denoiser_blend" => f.post_denoiser_blend,
        "post_denoiser_detail" => f.post_denoiser_detail,
        "post_denoiser_sharpen" => f.post_denoiser_sharpen,
        k if k.starts_with("effects_") => *effect_field(&mut f.effects.clone(), k)?,
        "frame_count" => f.animation.frame_count as f64,
        "fps" => f.animation.fps,
        "layer_count" => f.layers.len() as f64,
        "xform_count" => f.layers.first().map_or(0, |l| l.xforms.len()) as f64,
        "final_count" => f.layers.first().map_or(0, |l| l.final_xforms.len()) as f64,
        _ => return get_prop(f, key),
    })
}

/// The image effect `effects_<name>` (`effects_exposure`, …).
fn effect_field<'a>(fx: &'a mut rwf_core::render::effects::ImageEffects, key: &str) -> Option<&'a mut f64> {
    Some(match key.strip_prefix("effects_")? {
        "exposure" => &mut fx.exposure,
        "contrast" => &mut fx.contrast,
        "highlights" => &mut fx.highlights,
        "shadows" => &mut fx.shadows,
        "whites" => &mut fx.whites,
        "blacks" => &mut fx.blacks,
        "texture" => &mut fx.texture,
        "clarity" => &mut fx.clarity,
        "dehaze" => &mut fx.dehaze,
        "vignette" => &mut fx.vignette,
        "vignette_size" => &mut fx.vignette_size,
        "vignette_feather" => &mut fx.vignette_feather,
        "grain" => &mut fx.grain,
        "grain_size" => &mut fx.grain_size,
        _ => return None,
    })
}

fn set_flame_number(f: &mut Flame, key: &str, v: f64) -> Result<bool> {
    let count = |v: f64, what: &str| if v >= 1.0 { Ok(v.round() as u32) } else { err(format!("{what} must be at least 1")) };
    match key {
        "width" => f.width = count(v, "width")?,
        "height" => f.height = count(v, "height")?,
        "sample_density" => f.sample_density = v,
        "spatial_filter_radius" => f.spatial_filter_radius = v,
        "spatial_oversampling" => f.spatial_oversampling = count(v, "spatial_oversampling")?,
        "filter_sharpness" => f.filter_sharpness = v,
        "filter_low_density" => f.filter_low_density = v,
        "post_blur_radius" => f.post_blur_radius = v.round().max(0.0) as u32,
        "post_blur_fade" => f.post_blur_fade = v,
        "post_blur_falloff" => f.post_blur_falloff = v,
        "de_radius" => f.de_radius = v,
        "de_curve" => f.de_curve = v,
        "antialias_amount" => f.antialias_amount = v,
        "antialias_radius" => f.antialias_radius = v,
        "post_denoiser_blend" => f.post_denoiser_blend = v.clamp(0.0, 1.0),
        "post_denoiser_detail" => f.post_denoiser_detail = v.clamp(0.0, 1.0),
        "post_denoiser_sharpen" => f.post_denoiser_sharpen = v.clamp(0.0, 1.0),
        k if k.starts_with("effects_") => match effect_field(&mut f.effects, k) {
            Some(slot) => *slot = v,
            None => return Ok(set_prop(f, key, v)),
        },
        "frame_count" => f.animation.frame_count = count(v, "frame_count")?,
        "fps" => f.animation.fps = v,
        "layer_count" | "xform_count" | "final_count" => return err(format!("{key} is read-only (use add_/remove_ functions)")),
        _ => return Ok(set_prop(f, key, v)),
    }
    Ok(true)
}

impl UserData for LFlame {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("layer", |_, this, i: i64| this.layer(i));
        m.add_method("add_layer", |_, this, ()| {
            let mut f = this.0.borrow_mut();
            f.layers.push(Layer::default());
            Ok(LLayer { f: this.0.clone(), l: f.layers.len() - 1 })
        });
        m.add_method("remove_layer", |_, this, i: i64| {
            let mut f = this.0.borrow_mut();
            let i = index(i, f.layers.len(), "layer")?;
            if f.layers.len() == 1 {
                return err("a flame needs at least one layer");
            }
            f.layers.remove(i);
            Ok(())
        });
        // Shortcuts for the first layer.
        m.add_method("xform", |_, this, i: i64| this.first().xform(XFormKind::Normal, i));
        m.add_method("final_xform", |_, this, i: i64| this.first().xform(XFormKind::Final, i));
        m.add_method("add_xform", |_, this, (name, amount): (Option<String>, Option<f64>)| this.first().add(XFormKind::Normal, name, amount));
        m.add_method("add_final_xform", |_, this, (name, amount): (Option<String>, Option<f64>)| this.first().add(XFormKind::Final, name, amount));
        m.add_method("remove_xform", |_, this, i: i64| this.first().remove(XFormKind::Normal, i));
        m.add_method("remove_final_xform", |_, this, i: i64| this.first().remove(XFormKind::Final, i));
        m.add_method("random_gradient", |lua, this, g: Option<String>| this.first().random_gradient(lua, g));
        m.add_method("copy", |_, this, ()| Ok(LFlame::new(this.snapshot())));
        m.add_method("get", |_, this, path: String| match get_prop(&this.0.borrow(), &path) {
            Some(v) => Ok(v),
            None => err(format!("no property '{path}' (see flame:paths())")),
        });
        m.add_method("set", |_, this, (path, v): (String, f64)| {
            if set_prop(&mut this.0.borrow_mut(), &path, v) { Ok(()) } else { err(format!("no property '{path}' (see flame:paths())")) }
        });
        m.add_method("paths", |lua, this, ()| lua.create_sequence_from(all_props(&this.0.borrow()).into_iter().map(|(p, _)| p)));
        m.add_method("set_key", |_, this, (path, frame, v): (String, f64, f64)| {
            let mut f = this.0.borrow_mut();
            if get_prop(&f, &path).is_none() {
                return err(format!("no property '{path}' (see flame:paths())"));
            }
            f.animation.track_mut(&path).set_key(frame, v);
            Ok(())
        });
        m.add_method("clear_keys", |_, this, path: Option<String>| {
            let mut f = this.0.borrow_mut();
            match path {
                Some(p) => f.animation.tracks.retain(|t| t.path != p),
                None => f.animation.tracks.clear(),
            }
            Ok(())
        });
        m.add_method("set_background", |_, this, (r, g, b): (f64, f64, f64)| {
            let mut f = this.0.borrow_mut();
            f.background = rwf_core::model::Background { color: [channel(r), channel(g), channel(b)], ..Default::default() };
            Ok(())
        });
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| -> Result<Value> {
            let f = this.0.borrow();
            match key.as_str() {
                "name" => return lua.create_string(&f.name).map(Value::String),
                "bg_transparency" => return Ok(Value::Boolean(f.bg_transparency)),
                "solid" => return Ok(Value::Boolean(f.solid.enabled)),
                "post_denoiser_supersample" => return Ok(Value::Boolean(f.post_denoiser_supersample)),
                "post_denoiser_float" => return Ok(Value::Boolean(f.post_denoiser_float)),
                "post_denoiser_normals" => return Ok(Value::Boolean(f.post_denoiser_normals)),
                _ => {}
            }
            const INTEGERS: [&str; 8] =
                ["width", "height", "spatial_oversampling", "post_blur_radius", "frame_count", "layer_count", "xform_count", "final_count"];
            match flame_number(&f, &key) {
                Some(v) if INTEGERS.contains(&key.as_str()) => Ok(Value::Integer(v as i64)),
                Some(v) => Ok(Value::Number(v)),
                None => err(format!("a flame has no field '{key}'")),
            }
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, v): (String, Value)| {
            let mut f = this.0.borrow_mut();
            match key.as_str() {
                "name" => f.name = text(&v, "name")?,
                "bg_transparency" => f.bg_transparency = boolean(&v, &key)?,
                "solid" => f.solid.enabled = boolean(&v, &key)?,
                "post_denoiser_supersample" => f.post_denoiser_supersample = boolean(&v, &key)?,
                "post_denoiser_float" => f.post_denoiser_float = boolean(&v, &key)?,
                "post_denoiser_normals" => f.post_denoiser_normals = boolean(&v, &key)?,
                _ => {
                    if !set_flame_number(&mut f, &key, num(&v, &key)?)? {
                        return err(format!("a flame has no field '{key}'"));
                    }
                }
            }
            Ok(())
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let f = this.0.borrow();
            Ok(format!("flame '{}' ({} layer(s), {} transform(s))", f.name, f.layers.len(), f.layers.iter().map(|l| l.xforms.len()).sum::<usize>()))
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Layer

#[derive(Clone)]
pub struct LLayer {
    f: Rc<RefCell<Flame>>,
    l: usize,
}

impl LLayer {
    fn with<R>(&self, f: impl FnOnce(&Layer) -> Result<R>) -> Result<R> {
        match self.f.borrow().layers.get(self.l) {
            Some(l) => f(l),
            None => err(format!("layer {} no longer exists", self.l + 1)),
        }
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut Layer) -> Result<R>) -> Result<R> {
        match self.f.borrow_mut().layers.get_mut(self.l) {
            Some(l) => f(l),
            None => err(format!("layer {} no longer exists", self.l + 1)),
        }
    }

    fn xform(&self, kind: XFormKind, i: i64) -> Result<LXForm> {
        let n = self.with(|l| Ok(l.xforms_of(kind).len()))?;
        Ok(LXForm { f: self.f.clone(), l: self.l, kind, i: index(i, n, kind_name(kind))? })
    }

    fn add(&self, kind: XFormKind, name: Option<String>, amount: Option<f64>) -> Result<LXForm> {
        let mut xf = XForm::default();
        if let Some(name) = name {
            xf.variations.push(new_variation(&name, amount.unwrap_or(1.0))?);
        }
        let i = self.with_mut(|l| {
            let list = l.xforms_of_mut(kind);
            list.push(xf);
            Ok(list.len() - 1)
        })?;
        Ok(LXForm { f: self.f.clone(), l: self.l, kind, i })
    }

    fn remove(&self, kind: XFormKind, i: i64) -> Result<()> {
        self.with_mut(|l| {
            let i = index(i, l.xforms_of(kind).len(), kind_name(kind))?;
            l.xforms_of_mut(kind).remove(i);
            if kind == XFormKind::Normal {
                // Keep the xaos of the other transforms pointing at the same targets.
                for xf in l.xforms.iter_mut() {
                    if i < xf.chaos.len() {
                        xf.chaos.remove(i);
                    }
                }
            }
            Ok(())
        })
    }

    fn random_gradient(&self, lua: &Lua, generator: Option<String>) -> Result<()> {
        let g = match generator {
            Some(name) => find_by_label(GradientGenerator::ALL, |g| g.label(), &name, "gradient generator")?,
            None => GradientGenerator::All,
        };
        let state = lua.app_data_ref::<State>().ok_or_else(|| mlua::Error::runtime("no script state"))?.clone();
        let gradient = state.with_rng(|r| random_gradient_of(g, r));
        self.with_mut(|l| {
            l.gradient = gradient;
            Ok(())
        })
    }
}

fn kind_name(kind: XFormKind) -> &'static str {
    match kind {
        XFormKind::Normal => "transform",
        XFormKind::Final => "final transform",
        XFormKind::Background => "background transform",
    }
}

impl UserData for LLayer {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("xform", |_, this, i: i64| this.xform(XFormKind::Normal, i));
        m.add_method("final_xform", |_, this, i: i64| this.xform(XFormKind::Final, i));
        m.add_method("add_xform", |_, this, (name, amount): (Option<String>, Option<f64>)| this.add(XFormKind::Normal, name, amount));
        m.add_method("add_final_xform", |_, this, (name, amount): (Option<String>, Option<f64>)| this.add(XFormKind::Final, name, amount));
        m.add_method("remove_xform", |_, this, i: i64| this.remove(XFormKind::Normal, i));
        m.add_method("remove_final_xform", |_, this, i: i64| this.remove(XFormKind::Final, i));
        m.add_method("color", |_, this, i: i64| {
            this.with(|l| {
                let c = l.gradient.colors[index(i, l.gradient.colors.len(), "gradient color")?];
                Ok((c[0], c[1], c[2]))
            })
        });
        m.add_method("set_color", |_, this, (i, r, g, b): (i64, f64, f64, f64)| {
            this.with_mut(|l| {
                let i = index(i, l.gradient.colors.len(), "gradient color")?;
                l.gradient.colors[i] = [channel(r), channel(g), channel(b)];
                Ok(())
            })
        });
        m.add_method("set_gradient", |_, this, keys: Table| {
            // {{pos, r, g, b}, …} with pos 0..1, or {{r, g, b}, …} spread evenly.
            let mut points: Vec<(Option<f64>, [u8; 3])> = Vec::new();
            for k in keys.sequence_values::<Table>() {
                let k = k?;
                let v: Vec<f64> = k.sequence_values::<f64>().collect::<Result<_>>()?;
                points.push(match v.as_slice() {
                    [p, r, g, b] => (Some(*p), [channel(*r), channel(*g), channel(*b)]),
                    [r, g, b] => (None, [channel(*r), channel(*g), channel(*b)]),
                    _ => return err("set_gradient expects {pos, r, g, b} or {r, g, b} entries"),
                });
            }
            if points.is_empty() {
                return err("set_gradient needs at least one color");
            }
            let n = points.len();
            let keys: Vec<(f64, [u8; 3])> =
                points.iter().enumerate().map(|(i, (p, c))| (p.unwrap_or(if n == 1 { 0.0 } else { i as f64 / (n - 1) as f64 }), *c)).collect();
            this.with_mut(|l| {
                l.gradient = Gradient::from_keys(&keys);
                Ok(())
            })
        });
        m.add_method("random_gradient", |lua, this, g: Option<String>| this.random_gradient(lua, g));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| -> Result<Value> {
            this.with(|l| {
                Ok(match key.as_str() {
                    "name" => Value::String(lua.create_string(&l.name)?),
                    "visible" => Value::Boolean(l.visible),
                    "weight" => Value::Number(l.weight),
                    "density" => Value::Number(l.density),
                    "gradient_shift" => Value::Number(l.gradient_shift),
                    "xform_count" => Value::Integer(l.xforms.len() as i64),
                    "final_count" => Value::Integer(l.final_xforms.len() as i64),
                    "gradient_size" => Value::Integer(l.gradient.colors.len() as i64),
                    "index" => Value::Integer(this.l as i64 + 1),
                    _ => return err(format!("a layer has no field '{key}'")),
                })
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, v): (String, Value)| {
            this.with_mut(|l| {
                match key.as_str() {
                    "name" => l.name = text(&v, "name")?,
                    "visible" => l.visible = boolean(&v, &key)?,
                    "weight" => l.weight = num(&v, &key)?,
                    "density" => l.density = num(&v, &key)?,
                    "gradient_shift" => l.gradient_shift = num(&v, &key)?,
                    _ => return err(format!("a layer has no writable field '{key}'")),
                }
                Ok(())
            })
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| this.with(|l| Ok(format!("layer {} ({} transform(s))", this.l + 1, l.xforms.len()))));
    }
}

// ---------------------------------------------------------------------------------------------
// Transform

#[derive(Clone)]
pub struct LXForm {
    f: Rc<RefCell<Flame>>,
    l: usize,
    kind: XFormKind,
    i: usize,
}

impl LXForm {
    fn missing<R>(&self) -> Result<R> {
        err(format!("{} {} no longer exists", kind_name(self.kind), self.i + 1))
    }

    fn with<R>(&self, f: impl FnOnce(&XForm) -> Result<R>) -> Result<R> {
        match self.f.borrow().layers.get(self.l).and_then(|l| l.xforms_of(self.kind).get(self.i)) {
            Some(xf) => f(xf),
            None => self.missing(),
        }
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut XForm) -> Result<R>) -> Result<R> {
        let mut flame = self.f.borrow_mut();
        match flame.layers.get_mut(self.l).and_then(|l| l.xforms_of_mut(self.kind).get_mut(self.i)) {
            Some(xf) => f(xf),
            None => self.missing(),
        }
    }

    /// Property path of a field (`c00` -> `layer0.xf1.coefs.c00`).
    fn path(&self, key: &str) -> String {
        let rest = match key {
            k if k.len() == 3 && k.starts_with('c') => format!("coefs.{k}"),
            k if k.starts_with("post_c") => format!("post.{}", &k[5..]),
            k if k.starts_with("yz_c") || k.starts_with("zx_c") => format!("{}_coefs.{}", &k[..2], &k[3..]),
            k if k.starts_with("yz_post_c") || k.starts_with("zx_post_c") => format!("{}_post.{}", &k[..2], &k[8..]),
            k => k.to_string(),
        };
        format!("layer{}.{}{}.{rest}", self.l, self.kind.path_prefix(), self.i)
    }

    fn variation(&self, i: i64) -> Result<LVar> {
        let n = self.with(|xf| Ok(xf.variations.len()))?;
        Ok(LVar { xf: self.clone(), v: index(i, n, "variation")? })
    }

    fn affine_mut<R>(&self, post: bool, f: impl FnOnce(&mut Affine) -> R) -> Result<R> {
        self.with_mut(|xf| Ok(f(if post { &mut xf.post } else { &mut xf.coefs })))
    }
}

fn new_variation(name: &str, amount: f64) -> Result<VariationInstance> {
    if variations::find(name).is_none() {
        let close: Vec<&str> = variations::names().filter(|n| n.contains(name) || name.contains(*n) && n.len() > 3).take(8).collect();
        return if close.is_empty() {
            err(format!("unknown variation '{name}' (rwf.variations() lists all)"))
        } else {
            err(format!("unknown variation '{name}' (similar: {})", close.join(", ")))
        };
    }
    Ok(VariationInstance::new(name, amount))
}

fn xform_number(xf: &XForm, key: &str) -> Option<f64> {
    Some(match key {
        "mod_gamma" => xf.mod_gamma,
        "mod_gamma_speed" => xf.mod_gamma_speed,
        "mod_contrast" => xf.mod_contrast,
        "mod_contrast_speed" => xf.mod_contrast_speed,
        "mod_saturation" => xf.mod_saturation,
        "mod_saturation_speed" => xf.mod_saturation_speed,
        "mod_hue" => xf.mod_hue,
        "mod_hue_speed" => xf.mod_hue_speed,
        _ => return None,
    })
}

fn set_xform_number(xf: &mut XForm, key: &str, v: f64) -> bool {
    match key {
        "mod_gamma" => xf.mod_gamma = v,
        "mod_gamma_speed" => xf.mod_gamma_speed = v,
        "mod_contrast" => xf.mod_contrast = v,
        "mod_contrast_speed" => xf.mod_contrast_speed = v,
        "mod_saturation" => xf.mod_saturation = v,
        "mod_saturation_speed" => xf.mod_saturation_speed = v,
        "mod_hue" => xf.mod_hue = v,
        "mod_hue_speed" => xf.mod_hue_speed = v,
        _ => return false,
    }
    true
}

impl UserData for LXForm {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("variation", |_, this, i: i64| this.variation(i));
        m.add_method("find_variation", |_, this, name: String| {
            let i = this.with(|xf| Ok(xf.variations.iter().position(|v| v.name == name)))?;
            Ok(i.map(|v| LVar { xf: this.clone(), v }))
        });
        m.add_method("add_variation", |_, this, (name, amount): (String, Option<f64>)| {
            let var = new_variation(&name, amount.unwrap_or(1.0))?;
            let v = this.with_mut(|xf| {
                xf.variations.push(var);
                Ok(xf.variations.len() - 1)
            })?;
            Ok(LVar { xf: this.clone(), v })
        });
        m.add_method("remove_variation", |_, this, i: i64| {
            this.with_mut(|xf| {
                let i = index(i, xf.variations.len(), "variation")?;
                xf.variations.remove(i);
                Ok(())
            })
        });
        m.add_method("clear_variations", |_, this, ()| {
            this.with_mut(|xf| {
                xf.variations.clear();
                Ok(())
            })
        });
        m.add_method("rotate", |_, this, (deg, post): (f64, Option<bool>)| this.affine_mut(post.unwrap_or(false), |a| a.rotate(deg)));
        m.add_method("scale", |_, this, (f, post): (f64, Option<bool>)| this.affine_mut(post.unwrap_or(false), |a| a.scale(f)));
        m.add_method("translate", |_, this, (dx, dy, post): (f64, f64, Option<bool>)| {
            this.affine_mut(post.unwrap_or(false), |a| a.translate(dx, dy))
        });
        m.add_method("reset", |_, this, post: Option<bool>| this.affine_mut(post.unwrap_or(false), |a| *a = Affine::IDENTITY));
        m.add_method("chaos", |_, this, to: i64| this.with(|xf| Ok(xf.chaos_to(index(to, usize::MAX, "transform")?))));
        m.add_method("set_chaos", |_, this, (to, v): (i64, f64)| {
            this.with_mut(|xf| {
                let to = index(to, usize::MAX, "transform")?;
                if xf.chaos.len() <= to {
                    xf.chaos.resize(to + 1, 1.0);
                }
                xf.chaos[to] = v;
                Ok(())
            })
        });
        m.add_method("duplicate", |_, this, ()| {
            let copy = this.with(|xf| Ok(xf.clone()))?;
            let mut f = this.f.borrow_mut();
            let list = f.layers[this.l].xforms_of_mut(this.kind);
            list.push(copy);
            Ok(LXForm { f: this.f.clone(), l: this.l, kind: this.kind, i: list.len() - 1 })
        });
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| -> Result<Value> {
            match key.as_str() {
                "name" => return this.with(|xf| Ok(Value::String(lua.create_string(&xf.name)?))),
                "draw_mode" => {
                    return this.with(|xf| {
                        let s = match xf.draw_mode {
                            DrawMode::Normal => "normal",
                            DrawMode::Opaque => "opaque",
                            DrawMode::Hidden => "hidden",
                        };
                        Ok(Value::String(lua.create_string(s)?))
                    });
                }
                "index" => return Ok(Value::Integer(this.i as i64 + 1)),
                _ => {}
            }
            if key == "var_count" {
                return this.with(|xf| Ok(Value::Integer(xf.variations.len() as i64)));
            }
            if let Some(v) = this.with(|xf| Ok(xform_number(xf, &key)))? {
                return Ok(Value::Number(v));
            }
            match get_prop(&this.f.borrow(), &this.path(&key)) {
                Some(v) => Ok(Value::Number(v)),
                None => this.with(|_| err(format!("a transform has no field '{key}'"))),
            }
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, v): (String, Value)| {
            match key.as_str() {
                "name" => {
                    return this.with_mut(|xf| {
                        xf.name = text(&v, "name")?;
                        Ok(())
                    });
                }
                "draw_mode" => {
                    let mode = match text(&v, "draw_mode")?.as_str() {
                        "normal" => DrawMode::Normal,
                        "opaque" => DrawMode::Opaque,
                        "hidden" => DrawMode::Hidden,
                        other => return err(format!("draw_mode must be normal, opaque or hidden, not '{other}'")),
                    };
                    return this.with_mut(|xf| {
                        xf.draw_mode = mode;
                        Ok(())
                    });
                }
                _ => {}
            }
            let value = num(&v, &key)?;
            if this.with_mut(|xf| Ok(set_xform_number(xf, &key, value)))? {
                return Ok(());
            }
            let path = this.path(&key);
            if set_prop(&mut this.f.borrow_mut(), &path, value) {
                return Ok(());
            }
            this.with(|_| err(format!("a transform has no writable field '{key}'")))
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            this.with(|xf| {
                let vars: Vec<String> = xf.variations.iter().map(|v| format!("{} {}", v.name, v.amount)).collect();
                Ok(format!("{} {} [{}]", kind_name(this.kind), this.i + 1, vars.join(", ")))
            })
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Variation

#[derive(Clone)]
pub struct LVar {
    xf: LXForm,
    v: usize,
}

impl LVar {
    fn with<R>(&self, f: impl FnOnce(&VariationInstance) -> Result<R>) -> Result<R> {
        self.xf.with(|xf| match xf.variations.get(self.v) {
            Some(v) => f(v),
            None => err(format!("variation {} no longer exists", self.v + 1)),
        })
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut VariationInstance) -> Result<R>) -> Result<R> {
        self.xf.with_mut(|xf| match xf.variations.get_mut(self.v) {
            Some(v) => f(v),
            None => err(format!("variation {} no longer exists", self.v + 1)),
        })
    }

    fn param_names(v: &VariationInstance) -> String {
        let names: Vec<&str> = variations::find(&v.name).map(|d| d.params.iter().map(|p| p.name).collect()).unwrap_or_default();
        if names.is_empty() { format!("{} has no parameters", v.name) } else { format!("parameters of {}: {}", v.name, names.join(", ")) }
    }

    fn get(&self, name: &str) -> Result<f64> {
        self.with(|v| v.param(name).map_or_else(|| err(format!("no parameter '{name}' ({})", Self::param_names(v))), Ok))
    }

    fn set(&self, name: &str, value: f64) -> Result<()> {
        self.with_mut(|v| if v.set_param(name, value) { Ok(()) } else { err(format!("no parameter '{name}' ({})", Self::param_names(v))) })
    }
}

impl UserData for LVar {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("get", |_, this, name: String| this.get(&name));
        m.add_method("set", |_, this, (name, v): (String, f64)| this.set(&name, v));
        m.add_method("params", |lua, this, ()| {
            this.with(|v| {
                let t = lua.create_table()?;
                if let Some(def) = variations::find(&v.name) {
                    for (i, p) in def.params.iter().enumerate() {
                        t.set(p.name, v.params.get(i).copied().unwrap_or(p.default))?;
                    }
                }
                Ok(t)
            })
        });
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| -> Result<Value> {
            match key.as_str() {
                "name" => this.with(|v| Ok(Value::String(lua.create_string(&v.name)?))),
                "amount" => this.with(|v| Ok(Value::Number(v.amount))),
                "priority" => this.with(|v| Ok(Value::Integer(v.priority as i64))),
                _ => this.get(&key).map(Value::Number),
            }
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, v): (String, Value)| match key.as_str() {
            "name" => err("the variation of an entry cannot be changed (remove it and add another)"),
            "amount" => {
                let a = num(&v, "amount")?;
                this.with_mut(|var| {
                    var.amount = a;
                    Ok(())
                })
            }
            "priority" => {
                let p = num(&v, "priority")?.round() as i32;
                this.with_mut(|var| {
                    var.priority = p;
                    Ok(())
                })
            }
            _ => this.set(&key, num(&v, &key)?),
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| this.with(|v| Ok(format!("{} {}", v.name, v.amount))));
    }
}

// ---------------------------------------------------------------------------------------------
// The `rwf` module and globals

fn flame_arg(ud: &UserDataRef<LFlame>) -> LFlame {
    (**ud).clone()
}

fn labels<T: Copy>(lua: &Lua, all: &[T], label: impl Fn(T) -> &'static str) -> Result<Table> {
    lua.create_sequence_from(all.iter().map(|t| label(*t)))
}

pub fn install(lua: &Lua, state: &State, flame: Flame, params: &[(String, ParamValue)]) -> Result<()> {
    lua.set_app_data(state.clone());
    let g = lua.globals();
    g.set("flame", LFlame::new(flame))?;

    let p = lua.create_table()?;
    for (name, v) in params {
        match v {
            ParamValue::Number(n) if n.fract() == 0.0 && n.abs() < 9e15 => p.set(name.as_str(), *n as i64)?,
            ParamValue::Number(n) => p.set(name.as_str(), *n)?,
            ParamValue::Bool(b) => p.set(name.as_str(), *b)?,
            ParamValue::Text(s) => p.set(name.as_str(), s.as_str())?,
        }
    }
    g.set("params", p)?;

    let st = state.clone();
    g.set(
        "print",
        lua.create_function(move |lua, args: Variadic<Value>| {
            let tostring: mlua::Function = lua.globals().get("tostring")?;
            let parts: Vec<String> = args.into_iter().map(|a| tostring.call::<String>(a)).collect::<Result<_>>()?;
            st.print(parts.join("\t"));
            Ok(())
        })?,
    )?;

    let rwf = lua.create_table()?;
    rwf.set("version", env!("CARGO_PKG_VERSION"))?;
    rwf.set(
        "new_flame",
        lua.create_function(|_, ()| Ok(LFlame::new(Flame { name: "Script flame".into(), ..Flame::default() })))?,
    )?;
    let st = state.clone();
    rwf.set(
        "random_flame",
        lua.create_function(move |_, (generator, opts): (Option<String>, Option<Table>)| {
            let generator = match generator {
                Some(name) => find_by_label(Generator::ALL, |g| g.label(), &name, "generator")?,
                None => Generator::All,
            };
            let mut extras = Extras::NONE;
            if let Some(o) = opts {
                if let Some(s) = o.get::<Option<String>>("symmetry")? {
                    extras.symmetry = find_by_label(SymmetryGen::ALL, |g| g.label(), &s, "symmetry generator")?;
                }
                if let Some(s) = o.get::<Option<String>>("weighting")? {
                    extras.weighting = find_by_label(WeightingGen::ALL, |g| g.label(), &s, "weighting field generator")?;
                }
                if let Some(s) = o.get::<Option<String>>("gradient")? {
                    extras.gradient = find_by_label(GradientGenerator::ALL, |g| g.label(), &s, "gradient generator")?;
                }
            }
            Ok(LFlame::new(st.with_rng(|r| generate_with(generator, extras, r))))
        })?,
    )?;
    rwf.set("generators", lua.create_function(|lua, ()| labels(lua, Generator::ALL, |g| g.label()))?)?;
    rwf.set("symmetries", lua.create_function(|lua, ()| labels(lua, SymmetryGen::ALL, |g| g.label()))?)?;
    rwf.set("weightings", lua.create_function(|lua, ()| labels(lua, WeightingGen::ALL, |g| g.label()))?)?;
    rwf.set("gradient_generators", lua.create_function(|lua, ()| labels(lua, GradientGenerator::ALL, |g| g.label()))?)?;
    rwf.set("mutations", lua.create_function(|lua, ()| labels(lua, MutationType::ALL, |g| g.label()))?)?;
    let st = state.clone();
    rwf.set(
        "mutate",
        lua.create_function(move |_, (f, kind, strength): (UserDataRef<LFlame>, Option<String>, Option<f64>)| {
            let kind = match kind {
                Some(k) => find_by_label(MutationType::ALL, |m| m.label(), &k, "mutation")?,
                None => MutationType::All,
            };
            let f = flame_arg(&f);
            let mut flame = f.0.borrow_mut();
            st.with_rng(|r| mutate_flame(&mut flame, kind, strength.unwrap_or(1.0), r));
            Ok(())
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "seed",
        lua.create_function(move |_, seed: i64| {
            st.0.borrow_mut().rng = Rng::new(seed as u64);
            Ok(())
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "random",
        lua.create_function(move |_, (a, b): (Option<f64>, Option<f64>)| {
            let r = st.with_rng(|r| r.random());
            Ok(match (a, b) {
                (Some(lo), Some(hi)) => lo + r * (hi - lo),
                (Some(hi), None) => r * hi,
                _ => r,
            })
        })?,
    )?;
    rwf.set(
        "variations",
        lua.create_function(|lua, filter: Option<String>| {
            let mut names: Vec<&str> = variations::names().filter(|n| filter.as_ref().is_none_or(|f| n.contains(f.as_str()))).collect();
            names.sort_unstable();
            lua.create_sequence_from(names)
        })?,
    )?;
    rwf.set(
        "variation_params",
        lua.create_function(|lua, name: String| {
            let Some(def) = variations::find(&name) else { return err(format!("unknown variation '{name}'")) };
            let list = lua.create_table()?;
            for p in def.params {
                let t = lua.create_table()?;
                t.set("name", p.name)?;
                t.set("default", p.default)?;
                t.set("min", p.min)?;
                t.set("max", p.max)?;
                t.set("integer", p.integer)?;
                list.push(t)?;
            }
            Ok(list)
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "load_flames",
        lua.create_function(move |lua, path: String| {
            let p = st.path(&path);
            let flames = rwf_core::io::read_flames_from_file(&p).map_err(|e| mlua::Error::runtime(format!("cannot read {}: {e}", p.display())))?;
            lua.create_sequence_from(flames.into_iter().map(LFlame::new))
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "save_flame",
        lua.create_function(move |_, (f, path): (UserDataRef<LFlame>, String)| {
            let p = st.path(&path);
            rwf_core::io::write_flame_to_file(&f.snapshot(), &p).map_err(|e| mlua::Error::runtime(format!("cannot write {}: {e}", p.display())))
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "save_flames",
        lua.create_function(move |_, (list, path): (Table, String)| {
            let flames: Vec<Flame> = list.sequence_values::<AnyUserData>().map(|ud| Ok(ud?.borrow::<LFlame>()?.snapshot())).collect::<Result<_>>()?;
            let p = st.path(&path);
            std::fs::write(&p, rwf_core::io::write_flames(&flames)).map_err(|e| mlua::Error::runtime(format!("cannot write {}: {e}", p.display())))
        })?,
    )?;
    let st = state.clone();
    rwf.set(
        "output",
        lua.create_function(move |_, f: UserDataRef<LFlame>| {
            st.0.borrow_mut().outputs.push(f.snapshot());
            Ok(())
        })?,
    )?;
    g.set("rwf", rwf)?;
    Ok(())
}
