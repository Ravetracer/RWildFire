// SPDX-License-Identifier: LGPL-3.0-or-later

use super::*;
use rwf_core::model::{Layer, XForm};

fn base_flame() -> Flame {
    let layer = Layer { xforms: vec![XForm::with_variation("linear3D", 1.0), XForm::with_variation("julia", 0.5)], ..Layer::default() };
    Flame { name: "test".into(), layers: vec![layer], ..Flame::default() }
}

fn run_ok(src: &str) -> RunOutput {
    run(src, "test.lua", RunInput::new(base_flame())).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn run_err(src: &str) -> String {
    match run(src, "test.lua", RunInput::new(base_flame())) {
        Ok(_) => panic!("script should fail"),
        Err(e) => e.message,
    }
}

#[test]
fn header_params() {
    let h = Header::parse(
        "--- Title\n--- First line\n--- second line.\n--@param n integer 3 1 9 Count of things\n--@param s number 0.5\n--@param on bool true\n--@param c choice a|b|c Pick one\n--@param bad float 1\nprint(1)\n--@param late number 1\n",
    );
    assert_eq!(h.title, "Title");
    assert_eq!(h.description, "First line second line.");
    assert_eq!(h.params.len(), 4);
    assert_eq!(h.params[0].kind, ParamKind::Integer { min: 1, max: 9 });
    assert_eq!(h.params[0].help, "Count of things");
    assert_eq!(h.params[1].default, ParamValue::Number(0.5));
    assert_eq!(h.params[2].default, ParamValue::Bool(true));
    assert_eq!(h.params[3].kind, ParamKind::Choice(vec!["a".into(), "b".into(), "c".into()]));
    assert_eq!(h.params[3].default, ParamValue::Text("a".into()));
    assert_eq!(h.warnings.len(), 1, "{:?}", h.warnings);
    let v = h.values(&[("n".into(), "7".into()), ("c".into(), "B".into())]).unwrap();
    assert_eq!(v[0].1, ParamValue::Number(7.0));
    assert_eq!(v[3].1, ParamValue::Text("b".into()));
    assert!(h.values(&[("x".into(), "1".into())]).unwrap_err().contains("known: n, s, on, c"));
    assert!(h.values(&[("on".into(), "maybe".into())]).is_err());
}

#[test]
fn edits_current_flame() {
    let out = run_ok(
        r#"
        local xf = flame:xform(2)
        xf.weight = 3
        xf.c20 = 0.25
        xf.post_c21 = -0.5
        xf:variation(1).amount = 0.75
        local v = xf:add_variation("julian", 0.4)
        v.power = 5
        flame.brightness = 7
        flame.effects_vignette = -0.5
        flame.effects_clarity = flame.effects_vignette + 0.75
        flame.post_denoiser_detail = 0.9
        flame.width = 640
        flame.name = "changed"
        print("xforms", flame.xform_count)
    "#,
    );
    assert!(out.changed);
    let f = &out.flame;
    let xf = &f.layers[0].xforms[1];
    assert_eq!((xf.weight, xf.coefs.c20, xf.post.c21), (3.0, 0.25, -0.5));
    assert_eq!(xf.variations[0].amount, 0.75);
    assert_eq!(xf.variations[1].name, "julian");
    assert_eq!(xf.variations[1].param("power"), Some(5.0));
    assert_eq!((f.brightness, f.width, f.name.as_str()), (7.0, 640, "changed"));
    assert_eq!((f.effects.vignette, f.effects.clarity, f.post_denoiser_detail), (-0.5, 0.25, 0.9));
    assert_eq!(out.log, vec!["xforms\t2"]);
}

#[test]
fn unchanged_flame_is_reported() {
    let out = run_ok("local w = flame:xform(1).weight");
    assert!(!out.changed);
}

#[test]
fn replace_and_output_flames() {
    let out = run_ok(
        r#"
        local f = rwf.new_flame()
        local xf = f:add_xform("spherical", 1)
        xf:rotate(90)
        f:add_final_xform("bubble")
        rwf.output(f)
        local g = f:copy()
        g:add_layer():add_xform("linear3D")
        rwf.output(g)
        flame = g
    "#,
    );
    assert_eq!(out.flames.len(), 2);
    assert_eq!(out.flames[0].layers.len(), 1);
    assert_eq!(out.flames[1].layers.len(), 2);
    assert_eq!(out.flame, out.flames[1]);
    let a = &out.flames[0].layers[0].xforms[0].coefs;
    assert!((a.c00).abs() < 1e-12 && (a.c01.abs() - 1.0).abs() < 1e-12, "{a:?}");
}

#[test]
fn remove_and_chaos() {
    let out = run_ok(
        r#"
        flame:add_xform("julian")
        flame:xform(1):set_chaos(3, 0.5)
        flame:xform(1):set_chaos(2, 0)
        flame:remove_xform(2)
        assert(flame.xform_count == 2)
        assert(flame:xform(1):chaos(2) == 0.5)
    "#,
    );
    assert_eq!(out.flame.layers[0].xforms[0].chaos, vec![1.0, 0.5]);
}

#[test]
fn paths_and_keys() {
    let out = run_ok(
        r#"
        flame:set("layer0.xf0.var0.amount", 0.3)
        assert(math.abs(flame:get("layer0.xf0.var0.amount") - 0.3) < 1e-12)
        flame:set_key("cam_roll", 1, 0)
        flame:set_key("cam_roll", 50, 180)
        local found = false
        for _, p in ipairs(flame:paths()) do if p == "layer0.xf1.coefs.c00" then found = true end end
        assert(found)
    "#,
    );
    assert_eq!(out.flame.layers[0].xforms[0].variations[0].amount, 0.3);
    assert_eq!(out.flame.animation.track("cam_roll").unwrap().keys.len(), 2);
}

#[test]
fn gradient_functions() {
    let out = run_ok(
        r#"
        local l = flame:layer(1)
        l:set_gradient({{0, 0, 0}, {255, 0, 0}})
        local r, g, b = l:color(l.gradient_size)
        assert(r == 255 and g == 0 and b == 0)
        l:set_color(1, 10, 20, 30)
    "#,
    );
    assert_eq!(out.flame.layers[0].gradient.colors[0], [10, 20, 30]);
}

#[test]
fn generator_names_match_loosely() {
    run_ok("local f = rwf.random_flame('classic', {gradient = 'all'})\nassert(f.xform_count > 0)");
    let e = run_err("rwf.random_flame('nope')");
    assert!(e.contains("unknown generator 'nope'"), "{e}");
}

#[test]
fn params_reach_the_script() {
    let mut input = RunInput::new(base_flame());
    input.params = vec![("n".into(), ParamValue::Number(4.0)), ("on".into(), ParamValue::Bool(true)), ("c".into(), ParamValue::Text("b".into()))];
    let out = run("print(params.n, math.type(params.n), params.on, params.c)", "p.lua", input).unwrap();
    assert_eq!(out.log, vec!["4\tinteger\ttrue\tb"]);
}

#[test]
fn errors_name_the_script_line() {
    let e = run_err("local x = 1\nflame:xform(1):add_variation(\"no_such_var\")");
    assert!(e.starts_with("test.lua:2:") && e.contains("unknown variation 'no_such_var'"), "{e}");
    let e = run_err("flame:xform(5)");
    assert!(e.contains("transform 5 does not exist (1 to 2)"), "{e}");
    let e = run_err("flame.no_field = 1");
    assert!(e.contains("no field 'no_field'"), "{e}");
    let e = run_err("flame:xform(1):variation(1).nope = 2");
    assert!(e.contains("no parameter 'nope'"), "{e}");
    let e = run_err("local x = ");
    assert!(e.contains("test.lua:1:"), "{e}");
    let e = run_err("local xf = flame:xform(2)\nflame:remove_xform(2)\nprint(xf.weight)");
    assert!(e.contains("transform 2 no longer exists"), "{e}");
}

#[test]
fn cancel_stops_endless_loops() {
    let mut input = RunInput::new(base_flame());
    let cancel = input.cancel.clone();
    input.on_print = Some(Box::new(move |_| cancel.store(true, Ordering::Relaxed)));
    let e = run("print('go') while true do end", "loop.lua", input).unwrap_err();
    assert!(e.cancelled, "{}", e.message);
    assert_eq!(e.log, vec!["go"]);
}

#[test]
fn examples_run() {
    for (name, src) in EXAMPLES {
        let header = Header::parse(src);
        assert!(!header.title.is_empty() && header.warnings.is_empty(), "{name}: {header:?}");
        let mut given = Vec::new();
        if *name == "Random series.lua" {
            // Keep the test quick: random flames render coverage previews.
            given.push(("count".to_string(), "1".to_string()));
            given.push(("generator".to_string(), "Classic".to_string()));
            assert!(header.values(&[]).unwrap().contains(&("generator".to_string(), ParamValue::Text("All".into()))));
        }
        let mut input = RunInput::new(base_flame());
        input.params = header.values(&given).unwrap();
        let out = run(src, name, input).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(out.changed || !out.flames.is_empty(), "{name} did nothing");
    }
}

#[test]
fn reference_mentions_every_rwf_function() {
    let lua = Lua::new();
    api::install(&lua, &api::State::new(None, None), Flame::default(), &[]).unwrap();
    let rwf: mlua::Table = lua.globals().get("rwf").unwrap();
    for pair in rwf.pairs::<String, Value>() {
        let (k, _) = pair.unwrap();
        assert!(API_REFERENCE.contains(&format!("rwf.{k}")), "rwf.{k} is missing in API.md");
    }
}
