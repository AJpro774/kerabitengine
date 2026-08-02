//! Hello cube — Alpha smoke test for the public API.

use kerabit::prelude::*;

fn main() {
    Kerabit::new("Hello")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("cube")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(40.0))
                .material(Material::color(Color::GRAY))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(|ctx| {
            let dt = ctx.dt();
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }
            if let Some(cube) = ctx.world_mut().get_mut("cube") {
                cube.rotate_y(1.1 * dt);
            }
        });
}
