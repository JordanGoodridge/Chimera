//! What a view sees, surface by surface, and the texture mapping rules
//! found that way.
//!
//! `surfaces_in_view` (ignored: a debug aid for texture mismatches
//! reported from a playtest's `setpos`/`setang` line): rays from the eye
//! over a 1280x720 window's view (90 degrees across at 4:3) hit the map's
//! drawn meshes (world faces, displacements, overlays, brush entities)
//! and props; it prints a letter map of what each ray hit, then each
//! surface with its share of the view and what we resolved its material
//! to (base and blend textures, detail, alpha, unlit, tint, transform,
//! surface property). `MASHUP_VIEW_MAP=cs_militia MASHUP_VIEW_POS="261.04
//! -333.87 -94.43" MASHUP_VIEW_ANG="9.94 178.31 0" cargo test -p cs_source
//! --features dev --test it heavy::map_surfaces::surfaces_in_view --
//! --ignored --nocapture` (the position is `setpos`'s: the feet, the eye
//! 64 units above).
//!
//! `displacement_textures_follow_the_grid`: the rule that spot led to
//! (specs/cs_source/shaders.md open question 20).

use bevy::math::Vec3;

use crate::stack::{games::cs_source, map::MapMesh};

fn env_numbers(name: &str) -> Option<Vec<f32>> {
    let v = std::env::var(name).ok()?;
    Some(v.split_whitespace().filter_map(|x| x.parse().ok()).collect())
}

fn to_engine(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, z, -y) * cs_source::bsp::METERS_PER_UNIT
}

/// Möller-Trumbore: distance along `dir` to the triangle, if hit.
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4).then_some(t)
}

#[test]
#[ignore = "debug aid: MASHUP_VIEW_MAP, MASHUP_VIEW_POS, MASHUP_VIEW_ANG"]
fn surfaces_in_view() {
    let map_name = std::env::var("MASHUP_VIEW_MAP").unwrap_or_else(|_| "cs_militia".into());
    let pos = env_numbers("MASHUP_VIEW_POS").unwrap_or(vec![261.04, -333.87, -94.43]);
    let ang = env_numbers("MASHUP_VIEW_ANG").unwrap_or(vec![9.94, 178.31, 0.0]);
    let Ok(map) = crate::load_map(&format!("cs_source:{map_name}")) else {
        eprintln!("skipping: no {map_name}");
        return;
    };
    let eye = to_engine(pos[0], pos[1], pos[2] + 64.0);
    let (pitch, yaw) = (ang[0].to_radians(), ang[1].to_radians());
    let fwd_src = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), -pitch.sin());
    let forward = to_engine(fwd_src.x, fwd_src.y, fwd_src.z).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    // Triangles: (owner label, mesh, triangle) with world positions.
    struct Tri {
        owner: usize,
        p: [Vec3; 3],
    }
    let reach = 4000.0 * cs_source::bsp::METERS_PER_UNIT;
    // Every drawn mesh with its placement (rotation, then translation).
    let mut placed: Vec<(String, &MapMesh, bevy::math::Quat, Vec3)> = Vec::new();
    for m in map.meshes.iter().filter(|m| !m.skybox) {
        let (label, off) = match m.entity {
            Some(e) => {
                let o = map.entities[e].origin();
                (
                    format!("brush entity {e} ({})", map.entities[e].classname()),
                    to_engine(o.x, o.y, o.z),
                )
            }
            None => ("world".into(), Vec3::ZERO),
        };
        placed.push((label, m, bevy::math::Quat::IDENTITY, off));
    }
    for (i, prop) in map.props.iter().enumerate().filter(|(_, p)| !p.skybox) {
        if (prop.translation - eye).length() > reach {
            continue;
        }
        for m in &map.models[prop.model].meshes {
            placed.push((
                format!("prop {i} (model {})", prop.model),
                m,
                prop.rotation,
                prop.translation,
            ));
        }
    }
    let mut owners: Vec<(String, &MapMesh)> = Vec::new();
    let mut tris = Vec::new();
    for (label, mesh, rotation, translation) in placed {
        let owner = owners.len();
        let mut any = false;
        for t in mesh.indices.chunks(3) {
            let p = [0, 1, 2].map(|i| rotation * Vec3::from(mesh.positions[t[i] as usize]) + translation);
            if p.iter().all(|q| (*q - eye).length() > reach) {
                continue;
            }
            tris.push(Tri { owner, p });
            any = true;
        }
        if any {
            owners.push((label, mesh));
        }
    }
    // A 16:9 window's view (90 degrees across at 4:3, so wider), as a
    // screenshot at 1280x720 frames it.
    let (w, h) = (96usize, 54usize);
    let mut grid = vec![None; w * h];
    let mut hits = vec![0usize; owners.len()];
    let half = (45f32).to_radians().tan();
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f32 + 0.5) / w as f32 * 2.0 - 1.0) * half * 4.0 / 3.0;
            let sy = (1.0 - (y as f32 + 0.5) / h as f32 * 2.0) * half * 0.75;
            let dir = (forward + right * sx + up * sy).normalize();
            let mut best: Option<(f32, usize)> = None;
            for t in &tris {
                if let Some(d) = ray_triangle(eye, dir, t.p[0], t.p[1], t.p[2])
                    && best.is_none_or(|b| d < b.0)
                {
                    best = Some((d, t.owner));
                }
            }
            if let Some((_, o)) = best {
                hits[o] += 1;
                grid[y * w + x] = Some(o);
            }
        }
    }
    let mut order: Vec<usize> = (0..owners.len()).filter(|&o| hits[o] > 0).collect();
    order.sort_by_key(|&o| std::cmp::Reverse(hits[o]));
    // Where each surface is: a letter per surface, by share of the view.
    let letters: Vec<char> = ('A'..='Z').chain('a'..='z').collect();
    let letter = |o: usize| {
        order
            .iter()
            .position(|&x| x == o)
            .and_then(|i| letters.get(i))
            .copied()
            .unwrap_or('?')
    };
    for y in 0..h {
        let row: String = (0..w).map(|x| grid[y * w + x].map_or('.', letter)).collect();
        println!("{row}");
    }
    let name = |t: Option<usize>| t.map_or("-".to_string(), |t| map.textures[t].name.clone());
    for &o in &order {
        let (label, m) = &owners[o];
        println!(
            "{} {:5.1}% {label}: {} -> base {} normal {} blend {:?} detail {:?} alpha {:?} unlit {} tint {:?} transform {:?} surface {:?} envmap {} selfillum {} modulate {} frames {}",
            letter(o),
            hits[o] as f32 * 100.0 / (w * h) as f32,
            m.material,
            name(m.texture),
            name(m.normal_map),
            m.blend.map(|b| (name(b.texture), name(b.mask))),
            m.detail.map(|d| (name(Some(d.texture)), d.scale, d.factor, d.mode)),
            m.alpha,
            m.unlit,
            m.tint,
            (!m.base_transform.is_identity()).then_some(m.base_transform),
            m.surface,
            m.envmap.is_some(),
            m.selfillum.is_some(),
            m.modulate,
            m.frames.len(),
        );
    }
}

/// Displacements take their texture coordinates from the undisplaced
/// grid, not by projecting the sculpted vertices through the face's
/// texture axes: every vertex's texture point lies on the base face's
/// plane, between its corners, so a vertex moved sideways drags the
/// texture with it. cs_militia's rock walls move vertices up to 147
/// units sideways; projected, their texture smeared into streaks.
#[test]
fn displacement_textures_follow_the_grid() {
    use crate::stack::mount::config::LocalConfig;
    use bevy::math::Vec2;

    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(cs_source::GAME)) else {
        eprintln!("skipping: no CS:S install configured");
        return;
    };
    let Ok(mount) = cs_source::mount::open(&install) else {
        eprintln!("skipping: no CS:S install");
        return;
    };
    let Ok(bytes) = mount.read("maps/cs_militia.bsp") else {
        eprintln!("skipping: no cs_militia");
        return;
    };
    let bsp = vbsp::Bsp::read(&bytes).expect("cs_militia");
    let v3 = |v: vbsp::Vector| Vec3::new(v.x, v.y, v.z);
    let (mut vertices, mut dragged) = (0, 0);
    for face in bsp.models().next().unwrap().faces() {
        if face.displacement().is_none() {
            continue;
        }
        let tex = face.texture();
        let normal = v3(face.normal());
        let corners: Vec<Vec3> = face.vertices().map(|v| v3(v.position)).collect();
        let dist = normal.dot(corners[0]);
        let (lo, hi) = corners
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(a, b), c| (a.min(*c), b.max(*c)));
        for t in cs_source::bsp::face_triangles_blend(&face) {
            for (v, _, _, uv_at) in t {
                vertices += 1;
                let p = v3(uv_at);
                assert!(
                    (normal.dot(p) - dist).abs() < 1.0,
                    "texture point off the base face: {p} vs {corners:?} normal {normal}"
                );
                assert!(
                    p.cmpge(lo - 1.0).all() && p.cmple(hi + 1.0).all(),
                    "texture point outside the base face"
                );
                let projected = Vec2::from(tex.uv(v));
                let grid = Vec2::from(tex.uv(uv_at));
                if (projected - grid).length() > 1.0 / 32.0 {
                    dragged += 1;
                }
            }
        }
    }
    eprintln!("{dragged} of {vertices} displacement vertices map differently from a projection");
    assert!(vertices > 10_000 && dragged > vertices / 4, "{dragged} of {vertices}");
}
