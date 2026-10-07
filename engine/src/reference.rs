//! Settings recovered from Alto Adventure 1.8.24; see reference JSON for source objects.
//! Runtime reconstruction: 40 canvas units/source unit, jump strength interpreted as velocity,
//! flip/unwind factors interpreted as turns/s, landing size interpreted as a half-angle.
//! These interpretations remain hypotheses until measured against the running original.
pub const WORLD_SCALE: f64 = 40.;
pub const SOURCE_GRAVITY: f64 = 25.;
pub const FLIP_DELAY: f64 = 0.1;
#[derive(Clone, Copy)]
pub struct Profile {
    pub speed_min: f64,
    pub speed_max: f64,
    pub acceleration: f64,
    pub momentum: f64,
    pub boost_max: f64,
    pub boost_offset: f64,
    pub gravity_scale: f64,
    pub jump: f64,
    pub double_jump: f64,
    pub flip_factor: f64,
    // Recovered source fact retained for provenance; automatic leveling is disabled.
    #[allow(dead_code)]
    pub unwind_factor: f64,
    pub landing_degrees: f64,
    pub air_drag: f64,
    pub flip_drag: f64,
    pub double_jump_enabled: bool,
    pub chasm_rescues: u32,
}
pub const PROFILES: [Profile; 6] = [
    // Alto; Character object 2511
    Profile {
        speed_min: 20.,
        speed_max: 60.,
        acceleration: 0.800000012,
        momentum: 1.,
        boost_max: 2.,
        boost_offset: 0.,
        gravity_scale: 1.,
        jump: 16.,
        double_jump: 16.,
        flip_factor: 0.899999976,
        unwind_factor: 0.899999976,
        landing_degrees: 50.,
        air_drag: 0.5,
        flip_drag: 0.300000012,
        double_jump_enabled: false,
        chasm_rescues: 0,
    },
    // Maya; Character object 2512
    Profile {
        speed_min: 20.,
        speed_max: 60.,
        acceleration: 1.,
        momentum: 0.899999976,
        boost_max: 2.,
        boost_offset: 0.,
        gravity_scale: 0.850000024,
        jump: 16.,
        double_jump: 16.,
        flip_factor: 1.10000002,
        unwind_factor: 1.10000002,
        landing_degrees: 50.,
        air_drag: 0.550000012,
        flip_drag: 0.319999993,
        double_jump_enabled: false,
        chasm_rescues: 0,
    },
    // Paz; Character object 2485
    Profile {
        speed_min: 20.,
        speed_max: 56.,
        acceleration: 0.400000006,
        momentum: 1.5,
        boost_max: 2.,
        boost_offset: 0.,
        gravity_scale: 1.20000005,
        jump: 15.5,
        double_jump: 16.,
        flip_factor: 0.800000012,
        unwind_factor: 0.800000012,
        landing_degrees: 60.,
        air_drag: 0.5,
        flip_drag: 0.300000012,
        double_jump_enabled: false,
        chasm_rescues: 0,
    },
    // Izel; Character object 2486
    Profile {
        speed_min: 20.,
        speed_max: 68.,
        acceleration: 0.899999976,
        momentum: 1.,
        boost_max: 3.,
        boost_offset: 0.200000003,
        gravity_scale: 1.,
        jump: 16.,
        double_jump: 16.,
        flip_factor: 0.899999976,
        unwind_factor: 1.,
        landing_degrees: 50.,
        air_drag: 0.400000006,
        flip_drag: 0.200000003,
        double_jump_enabled: false,
        chasm_rescues: 0,
    },
    // Tupa; Character object 2474
    Profile {
        speed_min: 20.,
        speed_max: 62.,
        acceleration: 0.699999988,
        momentum: 1.20000005,
        boost_max: 2.5,
        boost_offset: 0.150000006,
        gravity_scale: 1.10000002,
        jump: 17.,
        double_jump: 16.,
        flip_factor: 1.25,
        unwind_factor: 1.,
        landing_degrees: 50.,
        air_drag: 0.5,
        flip_drag: 0.300000012,
        double_jump_enabled: true,
        chasm_rescues: 1,
    },
    // Felipe; Character object 2473
    Profile {
        speed_min: 20.,
        speed_max: 62.,
        acceleration: 0.800000012,
        momentum: 1.10000002,
        boost_max: 2.,
        boost_offset: 0.0799999982,
        gravity_scale: 1.,
        jump: 19.,
        double_jump: 17.,
        flip_factor: 0.899999976,
        unwind_factor: 1.,
        landing_degrees: 50.,
        air_drag: 0.5,
        flip_drag: 0.300000012,
        double_jump_enabled: true,
        chasm_rescues: 0,
    },
];
/// Exact continuous linear-drag step, shared by flight and guide prediction.
pub fn drag_step(v: f64, acceleration: f64, drag: f64, dt: f64) -> (f64, f64) {
    if drag.abs() < 1e-8 {
        return (v + acceleration * dt, v * dt + acceleration * dt * dt / 2.);
    }
    let decay = (-drag * dt).exp();
    let integral = -(-drag * dt).exp_m1() / drag;
    (
        v * decay + acceleration * integral,
        v * integral + acceleration / drag * (dt - integral),
    )
}
pub fn air_step(vx: f64, vy: f64, rider: usize, flipping: bool, dt: f64) -> (f64, f64, f64, f64) {
    let p = PROFILES[rider];
    let drag = if flipping { p.flip_drag } else { p.air_drag };
    // Preserve forward momentum; recovered drag is applied vertically only.
    let (nx, dx) = (vx, vx * dt);
    let (ny, dy) = drag_step(vy, SOURCE_GRAVITY * WORLD_SCALE * p.gravity_scale, drag, dt);
    (nx, ny, dx, dy)
}

/// Free-flight fit from 646 consecutive reference samples: drag .300/.295,
/// forward force 2.05 and downward force 27.19 (source units). Held input
/// rotates momentum, rather than imposing a climb velocity or clamping pitch.
pub fn wing_step(
    vx: f64,
    vy: f64,
    rotation: f64,
    ax: f64,
    ay: f64,
    dt: f64,
) -> (f64, f64, f64, f64) {
    let (nx, _) = drag_step(vx, 2.05 * WORLD_SCALE + ax, 0.3, dt);
    let (ny, _) = drag_step(vy, 27.19 * WORLD_SCALE + ay, 0.3, dt);
    let (s, c) = (rotation * dt).sin_cos();
    let (nx, ny) = (nx * c - ny * s, nx * s + ny * c);
    (nx, ny, (vx + nx) * dt / 2., (vy + ny) * dt / 2.)
}
