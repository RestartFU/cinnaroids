//! FakeLag's real-position marker: the feet position last relayed upstream, drawn as a
//! translucent coral player box with thin edges.
//!
//! Cinnabar reports the relayed position through `gameplay.read-relayed-position`; this
//! module smooths it for display and supplies the geometry the host previously drew, placed as
//! retained `world-draw` instances that move with the smoothed position.

use mod_api::bindings::cinnabar::world_draw::drawing::{
    Anchor, GeometrySpec, Handle, Instance, Mesh, Polyline, Vertex, WidthMode, WorldAnchor,
};

/// Standing player bounds around the feet, in blocks.
pub const HALF_WIDTH: f32 = 0.3;
pub const HEIGHT: f32 = 1.8;
/// Linear coral of the faces and edges.
const CORAL: [f32; 3] = [0.888, 0.195, 0.125];
const FACE_ALPHA: f32 = 0.12;
const EDGE_ALPHA: f32 = 0.9;
/// Edge thickness in world blocks, as the earlier host marker drew it.
pub const EDGE_WIDTH: f32 = 0.012;
/// Display convergence rate per second; matches at every frame rate.
const RATE: f32 = 40.0;
/// Squared distance beyond which a new sample is a teleport and snaps.
const SNAP_SQUARED: f32 = 64.0;
/// Retained instances: the face mesh and four edge paths.
pub const INSTANCES: usize = 5;

/// The displayed position, converging on each relayed sample without overshooting it.
#[derive(Default)]
pub struct Smoothed {
    session: u64,
    position: Option<[f32; 3]>,
}

impl Smoothed {
    /// Advances toward a session's sample. New sessions, teleports and re-enabled tracking snap;
    /// a missing or non-finite sample clears the display.
    pub fn update(&mut self, sample: Option<(u64, [f32; 3])>, seconds: f32) -> Option<[f32; 3]> {
        let Some((session, target)) = sample.filter(|(session, target)| {
            *session != 0 && target.iter().all(|value| value.is_finite())
        }) else {
            *self = Self::default();
            return None;
        };
        let position = match self.position {
            Some(previous)
                if self.session == session && distance_squared(previous, target) < SNAP_SQUARED =>
            {
                let weight = 1.0 - (-RATE * seconds.max(0.0)).exp();
                let point: [f32; 3] = std::array::from_fn(|axis| {
                    previous[axis] + (target[axis] - previous[axis]) * weight
                });
                if distance_squared(point, target) < 1e-8 {
                    target
                } else {
                    point
                }
            }
            _ => target,
        };
        self.session = session;
        self.position = Some(position);
        Some(position)
    }
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|axis| (a[axis] - b[axis]).powi(2)).sum()
}

/// Standing bounds at the feet, or none when the camera is inside them so a first-person view
/// is not tinted entirely.
pub fn bounds(feet: [f32; 3], camera: [f32; 3]) -> Option<[[f32; 3]; 2]> {
    let min = [feet[0] - HALF_WIDTH, feet[1], feet[2] - HALF_WIDTH];
    let max = [feet[0] + HALF_WIDTH, feet[1] + HEIGHT, feet[2] + HALF_WIDTH];
    let inside = (0..3).all(|axis| camera[axis] >= min[axis] && camera[axis] <= max[axis]);
    (!inside).then_some([min, max])
}

fn vertex(position: [f32; 3], alpha: f32) -> Vertex {
    Vertex {
        position: position.into(),
        color: (CORAL[0], CORAL[1], CORAL[2], alpha),
        uv: (0., 0.),
        data: (0., 0., 0., 0.),
    }
}

/// Box corners around the feet origin; bits select +x, +y and +z.
fn corner(index: usize) -> [f32; 3] {
    [
        if index & 1 == 0 {
            -HALF_WIDTH
        } else {
            HALF_WIDTH
        },
        if index & 2 == 0 { 0. } else { HEIGHT },
        if index & 4 == 0 {
            -HALF_WIDTH
        } else {
            HALF_WIDTH
        },
    ]
}

/// The translucent faces, then the twelve edges as four paths: both rings joined by one
/// vertical, and the other three verticals.
pub fn geometries() -> [GeometrySpec; INSTANCES] {
    let faces = GeometrySpec::Triangles(Mesh {
        vertices: (0..8)
            .map(|index| vertex(corner(index), FACE_ALPHA))
            .collect(),
        indices: [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ]
        .into_iter()
        .flat_map(|[a, b, c, d]| [a, b, c, a, c, d])
        .collect(),
    });
    let path = |corners: &[usize]| {
        GeometrySpec::Line(Polyline {
            vertices: corners
                .iter()
                .map(|index| vertex(corner(*index), EDGE_ALPHA))
                .collect(),
            closed: false,
        })
    };
    // Bottom ring 0-1-5-4, top ring 2-3-7-6, joined by the 0-2 vertical.
    [
        faces,
        path(&[0, 1, 5, 4, 0, 2, 3, 7, 6, 2]),
        path(&[1, 3]),
        path(&[5, 7]),
        path(&[4, 6]),
    ]
}

/// Depth-tested placements of the marker geometry at the displayed feet position.
pub fn instances(
    geometries: &[Handle; INSTANCES],
    dimension: i32,
    feet: [f32; 3],
) -> Vec<Instance> {
    geometries
        .iter()
        .enumerate()
        .map(|(index, geometry)| {
            let mut instance = mod_api::drawing::instance(
                *geometry,
                Anchor::World(WorldAnchor {
                    dimension,
                    origin: (f64::from(feet[0]), f64::from(feet[1]), f64::from(feet[2])),
                }),
            );
            if index > 0 {
                instance.width = EDGE_WIDTH;
                instance.width_mode = WidthMode::World;
            }
            instance
        })
        .collect()
}

/// The marker's retained instances and where they stand.
#[derive(Clone, Copy)]
pub struct Placed {
    pub handles: [Handle; INSTANCES],
    pub dimension: i32,
    pub feet: [f32; 3],
}

/// Marker drawing state: uploaded geometry, smoothing and the placed instances.
#[derive(Default)]
pub struct Marker {
    pub geometries: Option<[Handle; INSTANCES]>,
    /// Geometry uploads advance individually before the first placement batch.
    pub uploading: Vec<Handle>,
    pub smoothed: Smoothed,
    /// The retained instances while shown.
    pub placed: Option<Placed>,
}

impl Marker {
    /// The bounds' feet origin to draw this frame, if any. `sample` is the session and the
    /// relayed feet position; `camera` is the presented camera position.
    pub fn update(
        &mut self,
        sample: Option<(u64, [f32; 3])>,
        camera: [f32; 3],
        seconds: f32,
    ) -> Option<[f32; 3]> {
        let feet = self.smoothed.update(sample, seconds)?;
        bounds(feet, camera).map(|_| feet)
    }

    /// Drops all retained state after a failed drawing call so the next frame starts over.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
