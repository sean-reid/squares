/// sRGB (0..=255) to Oklab.
pub fn srgb_to_oklab(r: u8, g: u8, b: u8) -> [f64; 3] {
    let r = linearize(r);
    let g = linearize(g);
    let b = linearize(b);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

pub fn oklab_to_srgb(lab: [f64; 3]) -> [u8; 3] {
    let l_ = lab[0] + 0.396_337_777_4 * lab[1] + 0.215_803_757_3 * lab[2];
    let m_ = lab[0] - 0.105_561_345_8 * lab[1] - 0.063_854_172_8 * lab[2];
    let s_ = lab[0] - 0.089_484_177_5 * lab[1] - 1.291_485_548_0 * lab[2];
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_614_8 * m + 1.707_614_701_0 * s;
    [delinearize(r), delinearize(g), delinearize(b)]
}

fn linearize(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn delinearize(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_primaries() {
        for (r, g, b) in [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 255),
            (0, 0, 0),
            (120, 33, 200),
        ] {
            let back = oklab_to_srgb(srgb_to_oklab(r, g, b));
            assert!((back[0] as i32 - r as i32).abs() <= 1, "{:?}", back);
            assert!((back[1] as i32 - g as i32).abs() <= 1, "{:?}", back);
            assert!((back[2] as i32 - b as i32).abs() <= 1, "{:?}", back);
        }
    }
}
