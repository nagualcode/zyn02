//! `Misc/WaveShapeSmps.cpp`: the waveshaping functions shared by the
//! oscillator generator and the Distortion effect.

fn polyblampres(smp: f32, ws: f32, dmax: f32) -> f32 {
    if dmax == 0.0 {
        return 0.0;
    }
    let dist = smp.abs() - ws;
    let res = if dist.abs() < dmax {
        if dist < -dmax / 2.0 {
            let d1 = (dist + dmax) / dmax * 2.0;
            d1.powi(5) / 120.0
        } else if dist < 0.0 {
            let d1 = (dist + dmax / 2.0) / dmax * 2.0;
            let (d2, d3, d4, d5) = (d1 * d1, d1 * d1 * d1, d1.powi(4), d1.powi(5));
            -d5 / 40.0 + d4 / 24.0 + d3 / 12.0 + d2 / 12.0 + d1 / 24.0 + 1.0 / 120.0
        } else if dist < dmax / 2.0 {
            let d1 = dist / dmax * 2.0;
            let d2 = d1 * d1;
            let d4 = d2 * d2;
            let d5 = d4 * d1;
            d5 / 40.0 - d4 / 12.0 + d2 / 3.0 - d1 / 2.0 + 7.0 / 30.0
        } else {
            let d1 = (dist - dmax / 2.0) / dmax * 2.0;
            let (d2, d3, d4, d5) = (d1 * d1, d1 * d1 * d1, d1.powi(4), d1.powi(5));
            -d5 / 120.0 + d4 / 24.0 - d3 / 12.0 + d2 / 12.0 - d1 / 24.0 + 1.0 / 120.0
        }
    } else {
        0.0
    };
    res * dmax / 2.0
}

pub fn wave_shape_smps(smps: &mut [f32], ftype: u8, drive: u8, offset: u8, funcpar: u8) {
    let mut ws = drive as f32 / 127.0;
    let mut par = funcpar as f32 / 127.0;
    let offs = (offset as f32 - 64.0) / 64.0;
    match ftype {
        1 => {
            ws = 10f32.powf(ws * ws * 3.0) - 1.0 + 0.001;
            let d = ws.atan();
            for s in smps.iter_mut() {
                *s += offs;
                *s = (*s * ws).atan() / d;
                *s -= offs;
            }
        }
        2 => {
            ws = ws * ws * 32.0 + 0.0001;
            let tmpv = if ws < 1.0 { ws.sin() + 0.1 } else { 1.1 };
            for s in smps.iter_mut() {
                *s = (*s * (0.1 + ws - ws * *s)).sin() / tmpv;
            }
        }
        3 => {
            ws = ws * ws * ws * 20.0 + 0.0001;
            for s in smps.iter_mut() {
                *s *= ws;
                if s.abs() < 1.0 {
                    *s = (*s - *s * *s * *s) * 3.0;
                    if ws < 1.0 {
                        *s /= ws;
                    }
                } else {
                    *s = 0.0;
                }
            }
        }
        4 => {
            ws = ws * ws * ws * 32.0 + 0.0001;
            let tmpv = if ws < 1.57 { ws.sin() } else { 1.0 };
            for s in smps.iter_mut() {
                *s = (*s * ws).sin() / tmpv;
            }
        }
        5 => {
            ws = ws * ws + 0.000_001;
            for s in smps.iter_mut() {
                *s = (*s / ws + 0.5).floor() * ws;
            }
        }
        6 => {
            ws = ws * ws * ws * 32.0 + 0.0001;
            let tmpv = if ws < 1.0 { ws.sin() } else { 1.0 };
            for s in smps.iter_mut() {
                *s = (*s * ws).sin().asin() / tmpv;
            }
        }
        7 => {
            ws = 2f32.powf(-ws * ws * 8.0);
            par /= 4.0;
            if par as f64 > ws as f64 - 0.01 {
                par = (ws as f64 - 0.01) as f32;
            }
            for s in smps.iter_mut() {
                *s += offs;
                let res = polyblampres(*s, ws, par);
                *s = if *s >= 0.0 {
                    if *s > ws { ws - res } else { *s - res }
                } else if *s < -ws {
                    -ws + res
                } else {
                    *s + res
                };
                let res = polyblampres(offs, ws, par);
                if offs >= 0.0 {
                    *s -= if offs >= ws { ws - res } else { offs - res };
                } else {
                    *s -= if offs <= -ws { -ws + res } else { offs + res };
                }
                *s /= ws;
            }
        }
        8 => {
            ws = 2f32.powf(-ws * ws * 8.0);
            for s in smps.iter_mut() {
                if *s > ws {
                    *s = ws;
                }
                *s *= 2.0;
            }
        }
        9 => {
            ws = 2f32.powf(-ws * ws * 8.0);
            for s in smps.iter_mut() {
                if *s < -ws {
                    *s = -ws;
                }
                *s *= 2.0;
            }
        }
        10 => {
            ws = (2f32.powf(ws * 6.0) - 1.0) / 64.0;
            if par as f64 > ws as f64 - 0.01 {
                par = (ws as f64 - 0.01) as f32;
            }
            for s in smps.iter_mut() {
                *s += offs;
                let res = polyblampres(*s, ws, par);
                *s = if *s >= 0.0 {
                    if *s > ws { *s - ws + res } else { res }
                } else if *s < -ws {
                    *s + ws - res
                } else {
                    -res
                };
                *s -= offs;
            }
        }
        11 => {
            ws = 5f32.powf(ws * ws) - 1.0;
            for s in smps.iter_mut() {
                let v = *s * (ws + 0.5) * 0.9999;
                *s = v - (0.5 + v).floor();
            }
        }
        12 => {
            ws = ws * ws * ws * 30.0 + 0.001;
            let tmpv = if ws < 0.3 { ws } else { 1.0 };
            for s in smps.iter_mut() {
                let t = *s * ws;
                *s = if t > -2.0 && t < 1.0 { t * (1.0 - t) * (t + 2.0) / tmpv } else { 0.0 };
            }
        }
        13 => {
            ws = ws * ws * ws * 32.0 + 0.0001;
            let tmpv = if ws < 1.0 { ws * (1.0 + ws) / 2.0 } else { 1.0 };
            for s in smps.iter_mut() {
                let t = *s * ws;
                *s = if t > -1.0 && t < 1.618_034 {
                    t * (1.0 - t) / tmpv
                } else if t > 0.0 {
                    -1.0
                } else {
                    -2.0
                };
            }
        }
        14 => {
            ws = ws.powi(5) * 80.0 + 0.0001;
            let tmpv = if ws > 10.0 { 0.5 } else { 0.5 - 1.0 / (ws.exp() + 1.0) };
            for s in smps.iter_mut() {
                *s += offs;
                let t = (*s * ws).clamp(-10.0, 10.0);
                let t = 0.5 - 1.0 / (t.exp() + 1.0);
                let o = (offs * ws).clamp(-10.0, 10.0);
                let o = 0.5 - 1.0 / (o.exp() + 1.0);
                *s = t / tmpv - o / tmpv;
            }
        }
        15 => {
            par = 20.0 * par * par + 0.1 * par + 1.0;
            ws = ws * ws * 35.0 + 1.0;
            let off = offs / (1.0 + offs.abs().powf(par)).powf(1.0 / par);
            for s in smps.iter_mut() {
                *s *= ws;
                *s += offs;
                *s /= (1.0 + s.abs().powf(par)).powf(1.0 / par);
                *s -= off;
            }
        }
        16 => {
            ws = ws * ws * ws * 20.0 + 0.168;
            // Zyn evaluates the cubic in double precision.
            let off = 1.5 * (offs as f64 - (offs * offs * offs) as f64 / 3.0);
            for s in smps.iter_mut() {
                *s *= ws;
                *s += offs;
                *s = if s.abs() < 1.0 {
                    (1.5 * (*s as f64 - (*s * *s * *s) as f64 / 3.0)) as f32
                } else if *s > 0.0 {
                    1.0
                } else {
                    -1.0
                };
                *s = (*s as f64 - off) as f32;
            }
        }
        17 => {
            ws = ws * ws * ws * 20.0 + 0.168;
            let off = offs * (2.0 - offs.abs());
            for s in smps.iter_mut() {
                *s *= ws;
                *s += offs;
                *s = if s.abs() < 1.0 { *s * (2.0 - s.abs()) } else if *s > 0.0 { 1.0 } else { -1.0 };
                *s -= off;
            }
        }
        _ => {}
    }
}
