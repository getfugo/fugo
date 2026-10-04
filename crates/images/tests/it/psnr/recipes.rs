//! The recipes of the Go implementation's own golden images (`images_golden`).

use super::*;

/// The Go implementation's own golden images and their recipes (images_golden_integration_test.go),
/// the smart-anchor crops and fills included.
pub(super) fn go_golden_recipes() -> Vec<Recipe> {
    let sunset = "resources/testdata/sunset.jpg";
    let gopher = "resources/testdata/gopher-hero8.png";
    let mask = "resources/testdata/mask.png";
    let mask2 = "resources/testdata/mask2.png";
    let x300 = json!({"spec": "resize x300"});
    let mut recipes = Vec::new();
    let mut add = |golden: &str, source: &str, imaging: J, steps: J| {
        recipes.push(
            serde_json::from_value::<Recipe>(json!({
                "golden": golden, "source": source,
                "imaging": if imaging.is_null() { J::Null } else { imaging },
                "steps": steps,
            }))
            .expect("recipe"),
        );
    };
    let misc = |f: J| json!([x300, {"filters": f}]);
    for (name, filters) in [
        (
            "brightness-40.jpg",
            json!([{"op": "brightness", "percentage": 40}]),
        ),
        (
            "contrast-50.jpg",
            json!([{"op": "contrast", "percentage": 50}]),
        ),
        ("gamma-1.667.jpg", json!([{"op": "gamma", "gamma": 1.667}])),
        (
            "gaussianblur-5.jpg",
            json!([{"op": "gaussian_blur", "sigma": 5}]),
        ),
        ("grayscale.jpg", json!([{"op": "grayscale"}])),
        (
            "grayscale+colorize-180-50-20.jpg",
            json!([{"op": "grayscale"}, {"op": "colorize", "hue": 180, "saturation": 50, "percentage": 20}]),
        ),
        (
            "colorbalance-180-50-20.jpg",
            json!([{"op": "color_balance", "r": 180, "g": 50, "b": 20}]),
        ),
        ("hue--15.jpg", json!([{"op": "hue", "shift": -15}])),
        ("invert.jpg", json!([{"op": "invert"}])),
        (
            "opacity-0.65.jpg",
            json!([{"op": "opacity", "opacity": 0.65}]),
        ),
        (
            "padding-20-40-#976941.jpg",
            json!([{"op": "padding", "margin": [20, 40], "color": "#976941"}]),
        ),
        ("pixelate-10.jpg", json!([{"op": "pixelate", "size": 10}])),
        (
            "saturation-65.jpg",
            json!([{"op": "saturation", "percentage": 65}]),
        ),
        (
            "sigmoid-0.6--4.jpg",
            json!([{"op": "sigmoid", "midpoint": 0.6, "factor": -4}]),
        ),
        (
            "unsharpmask.jpg",
            json!([{"op": "unsharp_mask", "sigma": 10, "amount": 0.4, "threshold": 0.03}]),
        ),
    ] {
        add(
            &format!("filters/misc/{name}"),
            sunset,
            J::Null,
            misc(filters),
        );
    }
    add(
        "filters/misc/sepia-80.jpg",
        sunset,
        J::Null,
        json!([x300, {"filters": [{"op": "grayscale"}]}, {"filters": [{"op": "sepia", "percentage": 80}]}]),
    );
    add(
        "filters/misc/rotate270.jpg",
        "resources/testdata/exif/orientation6.jpg",
        J::Null,
        json!([{"filters": [{"op": "auto_orient"}]}]),
    );
    add(
        "filters/misc/text.jpg",
        sunset,
        J::Null,
        // The text of the Go test's golden image.
        misc(
            json!([{"op": "text", "text": "Hugo Rocks!", "color": "#fbfaf5",
            "linespacing": 8, "size": 40, "x": 25, "y": 190}]),
        ),
    );
    add(
        "filters/misc/dither-default.jpg",
        sunset,
        J::Null,
        misc(json!([{"op": "dither"}])),
    );
    // TestImagesGoldenFiltersText: the 900×562 sunset, text centred on (450, 281).
    let lorem = "Pariatur deserunt sunt nisi sunt tempor quis eu. Sint et nulla enim officia \
        sunt cupidatat. Eu amet ipsum qui velit cillum cillum ad Lorem in non ad aute.";
    let longer = "Est exercitation deserunt exercitation nostrud magna. Eiusmod anim deserunt \
        sit elit dolore ea incididunt nisi. Ea ullamco excepteur voluptate occaecat duis \
        pariatur proident cupidatat.  Eu id esse qui consectetur commodo ad ex esse cupidatat \
        velit duis cupidatat. Aliquip irure tempor consequat non amet in mollit ipsum officia \
        tempor laborum.";
    for (name, text, alignx, aligny) in [
        ("text_alignx-center.jpg", lorem, "center", "top"),
        ("text_alignx-right.jpg", lorem, "right", "top"),
        ("text_alignx-left.jpg", lorem, "left", "top"),
        (
            "text_alignx-center_aligny-center.jpg",
            longer,
            "center",
            "center",
        ),
        (
            "text_alignx-center_aligny-bottom.jpg",
            longer,
            "center",
            "bottom",
        ),
    ] {
        add(
            &format!("filters/text/{name}"),
            sunset,
            J::Null,
            json!([{"filters": [{"op": "text", "text": text, "color": "#fbfaf5",
                "linespacing": 8, "size": 28, "x": 450, "y": 281,
                "alignx": alignx, "aligny": aligny}]}]),
        );
    }
    // The overlay is the gopher resized to x80, itself an operation: run it separately below.
    let mask_cfg = |bg: &str| json!({"bgColor": bg, "hint": "photo", "quality": 75, "resampleFilter": "Lanczos"});
    for (name, spec, bg, m) in [
        (
            "filters/mask/transparant.png",
            "resize x300 png",
            "#ebcc34",
            mask,
        ),
        (
            "filters/mask/yellow.jpg",
            "resize x300 jpg",
            "#ebcc34",
            mask,
        ),
        ("filters/mask/wide.jpg", "resize 600x200", "#ebcc34", mask),
        (
            "filters/mask/blue.jpg",
            "resize x300 #323ea8",
            "#ebcc34",
            mask,
        ),
        (
            "filters/mask2/green.jpg",
            "resize x300 jpg",
            "#33ff44",
            mask,
        ),
        (
            "filters/mask2/pink.jpg",
            "resize x300 jpg",
            "#a83269",
            mask2,
        ),
    ] {
        add(
            name,
            sunset,
            mask_cfg(bg),
            json!([{"filters": [{"op": "process", "spec": spec}, {"op": "mask", "image": m}]}]),
        );
    }
    for (name, source, spec) in [
        (
            "process/misc/crop-500x200-smart.jpg",
            sunset,
            "crop 500x200 smart",
        ),
        (
            "process/misc/fill-500x200-smart.jpg",
            sunset,
            "fill 500x200 smart",
        ),
        (
            "process/misc/fit-500x200-smart.jpg",
            sunset,
            "fit 500x200 smart",
        ),
        (
            "process/misc/resize-100x100-r180.png",
            gopher,
            "resize 100x100 r180",
        ),
        (
            "process/misc/resize-300x300-jpg-b31280.jpg",
            gopher,
            "resize 300x300 jpg #b31280",
        ),
    ] {
        add(name, source, J::Null, json!([{"spec": spec}]));
    }
    let methods_cfg = json!({"bgColor": "#ebcc34", "hint": "photo", "quality": 75, "resampleFilter": "MitchellNetravali"});
    for (name, source, spec) in [
        ("methods/resize-sunsetjpg-300x.jpg", sunset, "resize 300x"),
        ("methods/resize-sunsetjpg-x200.jpg", sunset, "resize x200"),
        (
            "methods/fill-sunsetjpg-90x120-left.jpg",
            sunset,
            "fill 90x120 left",
        ),
        (
            "methods/fill-sunsetjpg-90x120-right.jpg",
            sunset,
            "fill 90x120 right",
        ),
        ("methods/fit-sunsetjpg-200x200.jpg", sunset, "fit 200x200"),
        // `.Crop "200x200"`: the default anchor, smart.
        ("methods/crop-sunsetjpg-200x200.jpg", sunset, "crop 200x200"),
        (
            "methods/crop-sunsetjpg-350x400-center.jpg",
            sunset,
            "crop 350x400 center",
        ),
        (
            "methods/crop-sunsetjpg-350x400-smart.jpg",
            sunset,
            "crop 350x400 smart",
        ),
        (
            "methods/crop-sunsetjpg-350x400-center-r90.jpg",
            sunset,
            "crop 350x400 center r90",
        ),
        (
            "methods/crop-sunsetjpg-350x400-center-q20.jpg",
            sunset,
            "crop 350x400 center q20",
        ),
        ("methods/resize-gopherpng-100x.png", gopher, "resize 100x"),
        (
            "methods/resize-gopherpng-100x-fc03ec.png",
            gopher,
            "resize 100x #fc03ec",
        ),
        (
            "methods/resize-gopherpng-100x-03fc56-jpg.jpg",
            gopher,
            "resize 100x #03fc56 jpg",
        ),
    ] {
        add(name, source, methods_cfg.clone(), json!([{"spec": spec}]));
    }
    recipes
}
