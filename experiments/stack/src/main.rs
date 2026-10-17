use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_triangle::float::triangulatable::Triangulatable;

type Contour = Vec<[f64; 2]>;
type Shape = Vec<Contour>;
type Shapes = Vec<Shape>;

fn main() {
    concave_overlay();
    polygon_with_hole();
    overlapping_union();
    self_intersection_polygonization();
    near_collinear_overlay();
    triangulate_concave_and_hole();
}

fn concave_overlay() {
    let subject = vec![vec![
        [0.0, 0.0],
        [4.0, 0.0],
        [4.0, 1.0],
        [1.0, 1.0],
        [1.0, 4.0],
        [0.0, 4.0],
    ]];
    let clip = vec![vec![[10.0, 0.0], [12.0, 0.0], [12.0, 2.0], [10.0, 2.0]]];
    let result = overlay(&subject, &clip, OverlayRule::Union);
    assert_near("concave union", area(&result), 11.0, 1e-9);
    println!(
        "concave union: {} shapes, area {:.12}",
        result.len(),
        area(&result)
    );
}

fn polygon_with_hole() {
    let subject = vec![
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]],
    ];
    let clip = vec![vec![[20.0, 0.0], [22.0, 0.0], [22.0, 2.0], [20.0, 2.0]]];
    let result = overlay(&subject, &clip, OverlayRule::Union);
    assert_near("union with hole", area(&result), 100.0, 1e-9);
    assert!(
        result.iter().any(|shape| shape.len() > 1),
        "hole was not retained"
    );
    println!(
        "union with hole: {} shapes, area {:.12}",
        result.len(),
        area(&result)
    );
}

fn overlapping_union() {
    let subject = vec![vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]];
    let clip = vec![vec![[2.0, 2.0], [6.0, 2.0], [6.0, 6.0], [2.0, 6.0]]];
    let result = overlay(&subject, &clip, OverlayRule::Union);
    assert_near("overlapping union", area(&result), 28.0, 1e-9);
    println!(
        "overlapping union: {} shapes, area {:.12}",
        result.len(),
        area(&result)
    );
}

fn self_intersection_polygonization() {
    let bow_tie = vec![vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 0.0]]];
    let empty = Shape::new();
    let result = overlay(&bow_tie, &empty, OverlayRule::Union);
    assert_near("bow-tie polygonization", area(&result), 2.0, 1e-9);
    println!(
        "bow-tie polygonization: {} shapes, area {:.12}",
        result.len(),
        area(&result)
    );
}

fn near_collinear_overlay() {
    let near_collinear = vec![vec![
        [0.0, 0.0],
        [1.0, 1e-10],
        [2.0, 0.0],
        [2.0, 2.0],
        [0.0, 2.0],
    ]];
    let disjoint = vec![vec![[4.0, 0.0], [5.0, 0.0], [5.0, 1.0], [4.0, 1.0]]];
    let result = overlay(&near_collinear, &disjoint, OverlayRule::Union);
    assert_near("near-collinear union", area(&result), 5.0, 1e-8);
    assert!(
        result
            .iter()
            .flatten()
            .flatten()
            .all(|point| { point[0].is_finite() && point[1].is_finite() })
    );
    println!(
        "near-collinear union: {} shapes, area {:.12}",
        result.len(),
        area(&result)
    );
}

fn triangulate_concave_and_hole() {
    let concave: Shape = vec![vec![
        [0.0, 0.0],
        [4.0, 0.0],
        [4.0, 1.0],
        [1.0, 1.0],
        [1.0, 4.0],
        [0.0, 4.0],
    ]];
    let concave_mesh = concave.triangulate().to_triangulation::<u32>();
    assert_near(
        "concave triangulation",
        mesh_area(&concave_mesh.points, &concave_mesh.indices),
        7.0,
        1e-8,
    );

    let with_hole: Shape = vec![
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]],
    ];
    let hole_mesh = with_hole.triangulate().to_triangulation::<u32>();
    assert_near(
        "hole triangulation",
        mesh_area(&hole_mesh.points, &hole_mesh.indices),
        96.0,
        1e-8,
    );

    println!(
        "triangulation: concave {} triangles / area {:.12}; hole {} triangles / area {:.12}",
        concave_mesh.indices.len() / 3,
        mesh_area(&concave_mesh.points, &concave_mesh.indices),
        hole_mesh.indices.len() / 3,
        mesh_area(&hole_mesh.points, &hole_mesh.indices),
    );
}

fn overlay(subject: &Shape, clip: &Shape, rule: OverlayRule) -> Shapes {
    subject.overlay(clip, rule, FillRule::EvenOdd)
}

fn area(shapes: &Shapes) -> f64 {
    shapes
        .iter()
        .flat_map(|shape| shape.iter())
        .map(signed_area)
        .sum()
}

fn signed_area(contour: &Contour) -> f64 {
    let mut sum = 0.0;
    for index in 0..contour.len() {
        let current = contour[index];
        let next = contour[(index + 1) % contour.len()];
        sum += current[0] * next[1] - next[0] * current[1];
    }
    sum * 0.5
}

fn mesh_area(points: &[[f64; 2]], indices: &[u32]) -> f64 {
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            let a = points[triangle[0] as usize];
            let b = points[triangle[1] as usize];
            let c = points[triangle[2] as usize];
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
        })
        .sum()
}

fn assert_near(name: &str, actual: f64, expected: f64, tolerance: f64) {
    assert!(actual.is_finite(), "{name} returned a non-finite area");
    assert!(
        (actual - expected).abs() <= tolerance,
        "{name} area {actual} differs from expected {expected} by more than {tolerance}"
    );
}
