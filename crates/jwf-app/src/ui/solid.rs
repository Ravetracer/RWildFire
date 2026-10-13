//! Solid rendering settings (render tab): lights, materials, ambient occlusion, shadows, bokeh.

use egui::{RichText, Ui};
use jwf_core::Flame;
use jwf_core::render::filter::FilterKernel;
use jwf_core::solid::{DistantLight, LightDiffFunc, Material, ShadowType};

use super::widgets::{self, grid, slider};
use crate::theme;

/// Color picker for linear `[f64; 3]` colors in 0..1.
fn color_f64(ui: &mut Ui, c: &mut [f64; 3]) {
    let mut v = [c[0] as f32, c[1] as f32, c[2] as f32];
    if egui::color_picker::color_edit_button_rgb(ui, &mut v).changed() {
        *c = [v[0] as f64, v[1] as f64, v[2] as f64];
    }
}

/// Filter / antialias settings replaced by the solid defaults, restored when solid rendering is
/// switched off again.
#[derive(Clone, Copy, PartialEq)]
struct FilterSettings {
    oversampling: u32,
    radius: f64,
    kernel: FilterKernel,
    aa_amount: f64,
    aa_radius: f64,
}

impl FilterSettings {
    /// `Flame.setDefaultSolidRenderingSettings`
    const SOLID: Self = Self { oversampling: 2, radius: 0.5, kernel: FilterKernel::Gaussian, aa_amount: 0.0, aa_radius: 0.0 };

    fn of(f: &Flame) -> Self {
        Self { oversampling: f.spatial_oversampling, radius: f.spatial_filter_radius, kernel: f.filter_kernel, aa_amount: f.antialias_amount, aa_radius: f.antialias_radius }
    }

    fn apply(&self, f: &mut Flame) {
        f.spatial_oversampling = self.oversampling;
        f.spatial_filter_radius = self.radius;
        f.filter_kernel = self.kernel;
        f.antialias_amount = self.aa_amount;
        f.antialias_radius = self.aa_radius;
    }
}

pub fn solid_section(ui: &mut Ui, f: &mut Flame) {
    theme::section_label(ui, "Solid rendering");
    let was_enabled = f.solid.enabled;
    ui.checkbox(&mut f.solid.enabled, "Render as solid surfaces (lights, shadows, ambient occlusion)");
    let stash_id = egui::Id::new("solid-filter-stash");
    if f.solid.enabled && !was_enabled {
        ui.data_mut(|d| d.insert_temp(stash_id, FilterSettings::of(f)));
        FilterSettings::SOLID.apply(f);
    } else if !f.solid.enabled && was_enabled {
        // Restore the previous filter settings unless they were edited meanwhile.
        let prev = ui.data_mut(|d| {
            let v = d.get_temp::<FilterSettings>(stash_id);
            d.remove::<FilterSettings>(stash_id);
            v
        });
        if let Some(prev) = prev
            && FilterSettings::of(f) == FilterSettings::SOLID
        {
            prev.apply(f);
        }
    }
    if !f.solid.enabled {
        return;
    }
    let s = &mut f.solid;

    egui::CollapsingHeader::new(RichText::new("Ambient occlusion").color(theme::TEXT_MUTED)).id_salt("sld-ao").show(ui, |ui| {
        ui.checkbox(&mut s.ao_enabled, "Enabled");
        if s.ao_enabled {
            grid(ui, "sld-ao-grid", |ui| {
                slider(ui, "Intensity", &mut s.ao_intensity, 0.0..=4.0);
                slider(ui, "Search radius", &mut s.ao_search_radius, 0.25..=30.0);
                slider(ui, "Blur radius", &mut s.ao_blur_radius, 0.25..=10.0);
                slider(ui, "Falloff", &mut s.ao_falloff, 0.0..=10.0);
                slider(ui, "Affect diffuse", &mut s.ao_affect_diffuse, 0.0..=1.0);
                widgets::label(ui, "Radius samples");
                ui.add(egui::Slider::new(&mut s.ao_radius_samples, 1..=32));
                ui.end_row();
                widgets::label(ui, "Azimuth samples");
                ui.add(egui::Slider::new(&mut s.ao_azimuth_samples, 1..=32));
                ui.end_row();
            });
        }
    });

    egui::CollapsingHeader::new(RichText::new("Shadows").color(theme::TEXT_MUTED)).id_salt("sld-shadows").show(ui, |ui| {
        grid(ui, "sld-sh-grid", |ui| {
            widgets::label(ui, "Type");
            egui::ComboBox::from_id_salt("sld-shadow-type").selected_text(s.shadow_type.label()).show_ui(ui, |ui| {
                for t in ShadowType::ALL {
                    ui.selectable_value(&mut s.shadow_type, *t, t.label());
                }
            });
            ui.end_row();
            if s.shadow_type != ShadowType::Off {
                widgets::label(ui, "Map size");
                ui.add(egui::Slider::new(&mut s.shadowmap_size, 256..=8192).logarithmic(true));
                ui.end_row();
                slider(ui, "Bias", &mut s.shadowmap_bias, 0.0..=0.2);
                if s.shadow_type == ShadowType::Smooth {
                    slider(ui, "Smoothness", &mut s.shadow_smooth_radius, 0.0..=10.0);
                }
            }
        });
        if s.shadow_type != ShadowType::Off && !s.lights.iter().any(|l| l.cast_shadows) {
            ui.label(RichText::new("No light casts shadows.").small().color(theme::WARN));
        }
    });

    egui::CollapsingHeader::new(RichText::new(format!("Lights ({})", s.lights.len())).color(theme::TEXT_MUTED)).id_salt("sld-lights").show(ui, |ui| {
        let mut remove = None;
        for (i, l) in s.lights.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Light {}", i + 1)).strong());
                color_f64(ui, &mut l.color);
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
            grid(ui, &format!("sld-light-{i}"), |ui| {
                slider(ui, "Altitude", &mut l.altitude, -90.0..=90.0);
                slider(ui, "Azimuth", &mut l.azimuth, -180.0..=180.0);
                slider(ui, "Intensity", &mut l.intensity, 0.0..=2.0);
                widgets::label(ui, "Shadows");
                ui.checkbox(&mut l.cast_shadows, "Cast shadows");
                ui.end_row();
                if l.cast_shadows {
                    slider(ui, "Shadow intensity", &mut l.shadow_intensity, 0.0..=1.0);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            s.lights.remove(i);
        }
        if ui.button("Add light").clicked() {
            s.lights.push(DistantLight::new_added());
        }
    });

    egui::CollapsingHeader::new(RichText::new(format!("Materials ({})", s.materials.len())).color(theme::TEXT_MUTED)).id_salt("sld-materials").show(ui, |ui| {
        ui.label(RichText::new("Transforms pick materials by index (\"Material\" in the transform tab); fractional indices blend neighbours.").small().color(theme::TEXT_MUTED));
        let mut remove = None;
        let n = s.materials.len();
        for (i, m) in s.materials.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Material {i}")).strong());
                if n > 1 && ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
            grid(ui, &format!("sld-mat-{i}"), |ui| {
                slider(ui, "Ambient", &mut m.ambient, 0.0..=2.0);
                slider(ui, "Diffuse", &mut m.diffuse, 0.0..=2.0);
                widgets::label(ui, "Diffuse response");
                egui::ComboBox::from_id_salt(format!("sld-mat-diff-{i}")).selected_text(m.light_diff_func.label()).show_ui(ui, |ui| {
                    for d in LightDiffFunc::ALL {
                        ui.selectable_value(&mut m.light_diff_func, *d, d.label());
                    }
                });
                ui.end_row();
                slider(ui, "Specular", &mut m.phong, 0.0..=2.0);
                slider(ui, "Specular size", &mut m.phong_size, 1.0..=200.0);
                widgets::label(ui, "Specular color");
                color_f64(ui, &mut m.phong_color);
                ui.end_row();
                widgets::label(ui, "Reflection map");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut m.refl_map_filename).desired_width(140.0).hint_text("PNG / JPEG file"));
                    if ui.small_button("…").clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
                    {
                        m.refl_map_filename = p.display().to_string();
                    }
                });
                ui.end_row();
                if !m.refl_map_filename.is_empty() {
                    slider(ui, "Reflection", &mut m.refl_map_intensity, 0.0..=2.0);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            s.materials.remove(i);
        }
        if ui.button("Add material").clicked() {
            s.materials.push(Material::new_added());
        }
    });

    if f.cam_dof > jwf_core::math::EPSILON {
        let s = &mut f.solid;
        egui::CollapsingHeader::new(RichText::new("Depth of field bokeh").color(theme::TEXT_MUTED)).id_salt("sld-bokeh").show(ui, |ui| {
            grid(ui, "sld-bokeh-grid", |ui| {
                widgets::label(ui, "Kernel");
                egui::ComboBox::from_id_salt("sld-bokeh-kernel").selected_text(s.post_bokeh_filter_kernel.label()).show_ui(ui, |ui| {
                    for k in FilterKernel::ALL {
                        ui.selectable_value(&mut s.post_bokeh_filter_kernel, *k, k.label());
                    }
                });
                ui.end_row();
                slider(ui, "Intensity", &mut s.post_bokeh_intensity, 0.0..=0.1);
                slider(ui, "Brightness", &mut s.post_bokeh_brightness, 0.0..=4.0);
                slider(ui, "Size", &mut s.post_bokeh_size, 0.0..=8.0);
                slider(ui, "Activation", &mut s.post_bokeh_activation, 0.0..=1.0);
            });
        });
    }
}
