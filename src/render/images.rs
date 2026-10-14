//! Images built from map data: textures, cubemaps and lightmaps (Source's
//! LDR lightmap encoding).

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

#[allow(unused_imports)]
use crate::map::*;

/// Source LDR lightmaps (specs/cs_source/shaders.md): each linear value L is
/// rounded to 1/1024, clamped to 4095/1024, stored as the 8-bit sRGB texel
/// 0.5 L^(1/2.2), and multiplied back by 2^2.2 after the sampler's decode.
pub(crate) const SOURCE_LIGHTMAP_SCALE: f32 = 4.594_793;

pub fn source_ldr_texel(l: f32) -> u8 {
    SOURCE_LDR.texel[source_ldr_level(l)]
}

/// The 1/1024 step a linear lightmap value is stored at (0-4095).
pub(crate) fn source_ldr_level(l: f32) -> usize {
    (l * 1024.0).round().clamp(0.0, 4095.0) as usize
}

/// Every level's encoding, worked out once: a `powf` per channel was most
/// of the cost of relighting blocks of animated light styles.
pub(crate) struct SourceLdrTable {
    /// The stored 8-bit value of each level.
    pub(crate) texel: [u8; 4096],
    /// Each level's 0.5 L^(1/2.2), before the 8-bit rounding.
    pub(crate) gamma: [f32; 4096],
}

pub(crate) static SOURCE_LDR: std::sync::LazyLock<SourceLdrTable> = std::sync::LazyLock::new(|| {
    let mut t = SourceLdrTable {
        texel: [0; 4096],
        gamma: [0.0; 4096],
    };
    for i in 0..4096 {
        let l = i as f32 / 1024.0;
        t.texel[i] = (255.0 * 0.5 * l.powf(1.0 / 2.2)).round() as u8;
        t.gamma[i] = 0.5 * l.powf(1.0 / 2.2);
    }
    t
});

/// Source LDR bump pages (specs/cs_source/shaders.md, "Bump page encoding
/// at upload"): the three directional values of a luxel (linear) are
/// scaled so their mean is the flat value's encoded level, overflow above 1
/// is shared out to the other pages, and the result is stored linearly
/// (the sampler's sRGB decode x 2^2.2 still applies when reading).
pub fn source_ldr_bump_texels(flat: [f32; 3], pages: [[f32; 3]; 3]) -> [[u8; 3]; 3] {
    let mut q = [[0.0f32; 3]; 3];
    for c in 0..3 {
        let goal = SOURCE_LDR.gamma[source_ldr_level(flat[c])];
        let mean = (pages[0][c] + pages[1][c] + pages[2][c]) / 3.0;
        let s = if mean > 0.0 { goal / mean } else { 0.0 };
        for k in 0..3 {
            q[k][c] = pages[k][c] * s;
        }
    }
    let max = |v: [f32; 3]| v[0].max(v[1]).max(v[2]);
    // Order once by largest channel, descending; ties go to the last
    // matching permutation in this list.
    const ORDERS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let m = q.map(max);
    let order = ORDERS
        .iter()
        .rev()
        .find(|o| m[o[0]] >= m[o[1]] && m[o[1]] >= m[o[2]])
        .copied()
        .unwrap_or([0, 1, 2]);
    for &k in &order {
        let top = max(q[k]);
        if top > 1.0 {
            let excess = q[k].map(|v| v * (top - 1.0) / top);
            for c in 0..3 {
                q[k][c] -= excess[c];
                for other in (0..3).filter(|&o| o != k) {
                    q[other][c] += excess[c] / 2.0;
                }
            }
        }
    }
    q.map(|v| {
        let top = max(v);
        let v = if top > 1.0 { v.map(|x| x / top) } else { v };
        v.map(|x| (255.0 * x.max(0.0)).round().clamp(0.0, 255.0) as u8)
    })
}

/// A repeating, mipmapped GPU image. Mipmaps are box-filtered here; source
/// files' own mip levels aren't used yet.
pub(crate) fn to_image(t: &MapTexture, look: &MapLook) -> Image {
    let mut data = t.rgba8.clone();
    let (mut w, mut h) = (t.width as usize, t.height as usize);
    let mut level = t.rgba8.clone();
    let mut levels = 1;
    // Prefer the texture's own mip levels (the original tools' filtering).
    let complete = (t.width.max(t.height) as f32).log2() as usize;
    if !t.mips.is_empty() && t.mips.len() == complete {
        for m in &t.mips {
            data.extend_from_slice(m);
        }
        levels += t.mips.len() as u32;
        w = 1;
        h = 1;
    }
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        sum += level[(sy * w + sx) * 4 + c] as u32;
                    }
                    next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        (w, h) = (nw, nh);
        levels += 1;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: t.width,
            height: t.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        if t.srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: if look.trilinear {
            ImageFilterMode::Linear
        } else {
            ImageFilterMode::Nearest
        },
        anisotropy_clamp: look.anisotropy.max(1),
        ..default()
    });
    image
}

/// Sample `image` clamped to its edges (pictures drawn once, not tiled).
pub(crate) fn clamp_edges(image: &mut Image) {
    if let ImageSampler::Descriptor(d) = &mut image.sampler {
        d.address_mode_u = ImageAddressMode::ClampToEdge;
        d.address_mode_v = ImageAddressMode::ClampToEdge;
    }
}

/// A baked cubemap as a cube texture (six sRGB layers with their mips,
/// clamped, linear within a mip, the nearest mip: the reference client's
/// bilinear filtering). The mips widen a bumped surface's highlight where
/// its normal varies faster than the screen's pixels, as the game's do.
pub fn cube_image(c: &MapCubemap) -> Image {
    // Layer by layer, each layer's mips largest first (wgpu's default
    // layer-major order).
    let mut image = Image::new_uninit(
        Extent3d {
            width: c.size,
            height: c.size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Comparison hook: MASHUP_CUBEMAP_TOP_MIP=1 keeps the largest mip only.
    if std::env::var("MASHUP_CUBEMAP_TOP_MIP").is_ok_and(|v| v == "1") {
        let top = (c.size * c.size * 4) as usize;
        image.data = Some(
            c.faces
                .iter()
                .flat_map(|f| f[..top.min(f.len())].iter().copied())
                .collect(),
        );
    } else {
        image.data = Some(c.faces.concat());
        image.texture_descriptor.mip_level_count = c.mips.max(1);
    }
    image.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A lighting atlas layer as a filterable half-float texture, clamped.
/// A lighting atlas layer in Source's LDR encoding (see
/// `SOURCE_LIGHTMAP_SCALE`): 8-bit sRGB, decoded by the sampler.
pub(crate) fn source_lightmap_image(rgb: &[[f32; 3]], width: u32, height: u32) -> Image {
    srgb8_image(source_ldr_texels(rgb), width, height)
}

/// RGBA8 sRGB texels as a linearly filtered lightmap layer.
pub(crate) fn srgb8_image(data: Vec<u8>, width: u32, height: u32) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// The lightmap atlas as images: the plain copy (Bevy's `Lightmap` on
/// StandardMaterial parts), the world material's copy (in the game's own
/// encoding when `source_ldr`) and its directional pages.
pub(crate) struct LightmapLayers {
    pub(crate) plain: Image,
    pub(crate) world: Image,
    pub(crate) bumped: Option<[Image; 3]>,
}

impl LightmapLayers {
    pub(crate) fn build(
        rgb: &[[f32; 3]],
        bumped: Option<&[Vec<[f32; 3]>; 3]>,
        width: u32,
        height: u32,
        source_ldr: bool,
    ) -> Self {
        let world_layer = |rgb: &[[f32; 3]]| {
            if source_ldr {
                source_lightmap_image(rgb, width, height)
            } else {
                lightmap_image(rgb, width, height)
            }
        };
        let pages = bumped.map(|b| {
            if source_ldr {
                // The game encodes the three pages together, against the flat one.
                source_ldr_page_texels(rgb, b).map(|t| srgb8_image(t, width, height))
            } else {
                std::array::from_fn(|i| world_layer(&b[i]))
            }
        });
        Self {
            plain: lightmap_image(rgb, width, height),
            world: world_layer(rgb),
            bumped: pages,
        }
    }
}

pub(crate) fn lightmap_image(rgb: &[[f32; 3]], width: u32, height: u32) -> Image {
    let data = f16_texels(rgb);
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Texel bytes of the plain lightmap layer (`lightmap_image`'s format).
pub(crate) fn f16_texels(rgb: &[[f32; 3]]) -> Vec<u8> {
    let mut data = Vec::with_capacity(rgb.len() * 8);
    for [r, g, b] in rgb {
        for c in [*r, *g, *b, 1.0] {
            data.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
        }
    }
    data
}

/// Texel bytes of the world layer in Source's LDR encoding
/// (`source_lightmap_image`'s format).
pub(crate) fn source_ldr_texels(rgb: &[[f32; 3]]) -> Vec<u8> {
    let mut data = Vec::with_capacity(rgb.len() * 4);
    for [r, g, b] in rgb {
        data.extend([source_ldr_texel(*r), source_ldr_texel(*g), source_ldr_texel(*b), 255]);
    }
    data
}

/// Texel bytes of the three directional pages in Source's LDR encoding.
pub(crate) fn source_ldr_page_texels(flat: &[[f32; 3]], pages: &[Vec<[f32; 3]>; 3]) -> [Vec<u8>; 3] {
    let mut texels: [Vec<u8>; 3] = std::array::from_fn(|_| Vec::with_capacity(flat.len() * 4));
    for (i, f) in flat.iter().enumerate() {
        let enc = source_ldr_bump_texels(*f, [pages[0][i], pages[1][i], pages[2][i]]);
        for (t, [r, g, b]) in texels.iter_mut().zip(enc) {
            t.extend([r, g, b, 255]);
        }
    }
    texels
}
