use meshcraft_core::{
    document::{
        Document, OccurrenceRef,
        architecture::{Control, Element, Wall},
    },
    geometry::Vec3,
    selection::{SelectedEntity, SelectionKind, SelectionTarget},
};
use std::{hint::black_box, time::Instant};
fn main() {
    for count in [0, 300] {
        let mut d = Document::new();
        for i in 0..count {
            d.add_cuboid(
                "background",
                Vec3::new(1., 2., 1.),
                Vec3::new((i % 20) as f64 * 2., 0., (i / 20) as f64 * 2.),
            )
            .unwrap();
        }
        let id = d
            .put_architecture(
                Document::SCENE_ROOT_DEFINITION_ID,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 6.,
                    auto_join: false,
                    ..Default::default()
                }),
            )
            .unwrap();
        let target = SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Wall,
                id,
            },
        };
        let e = d.architecture_element(&target).unwrap().element;
        let control = Control::WallBase { end: true };
        let t = Instant::now();
        for i in 0..100 {
            let p = Vec3::new(6., 1. + i as f64 * 0.01, 2.);
            let mut preview = d.clone();
            preview
                .move_architecture_control(&target, &control, p)
                .unwrap();
            black_box(preview);
        }
        let old = t.elapsed().as_secs_f64() * 1000.;
        let t = Instant::now();
        for i in 0..100 {
            let p = Vec3::new(6., 1. + i as f64 * 0.01, 2.);
            black_box(e.with_control_at(&control, p).unwrap().mesh().unwrap());
        }
        let new = t.elapsed().as_secs_f64() * 1000.;
        println!(
            "background_cuboids={count}, moves=100, old_document_preview_ms={old:.3}, owner_wireframe_ms={new:.3}, ratio={:.2}",
            old / new
        );
    }
}
