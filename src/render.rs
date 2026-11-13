//! WGPU batched UI renderer and `ab_glyph` shelf-packed glyph atlas.

use std::collections::HashMap;

use ab_glyph::{point, Font, FontArc, GlyphId, PxScale, ScaleFont};
use bytemuck::{Pod, Zeroable};

use crate::geom::Size;
use crate::layout::TextMeasurer;
use crate::runtime::{DrawList, QuadPrimitive, TextPrimitive};

const ATLAS_SIZE: u32 = 1024;
const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/Hack-Regular.ttf");

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GlobalsUniform {
    screen_size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    rect_size: [f32; 2],
    bg_color: [f32; 4],
    border_color: [f32; 4],
    params: [f32; 4],
}

impl GpuVertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
            2 => Float32x2,
            3 => Float32x4,
            4 => Float32x4,
            5 => Float32x4,
        ],
    };
}

#[derive(Debug, Clone, Copy)]
struct CachedGlyph {
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    offset_x: f32,
    offset_y: f32,
    width: f32,
    height: f32,
}

/// CPU glyph rasterizer and shelf-packed `R8Unorm` texture atlas manager.
pub struct FontSystem {
    font: FontArc,
    cache: HashMap<(GlyphId, u32), CachedGlyph>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl FontSystem {
    pub fn new() -> Self {
        let font = FontArc::try_from_slice(FONT_BYTES).expect("embedded Hack-Regular.ttf is valid");
        Self {
            font,
            cache: HashMap::new(),
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
        }
    }

    fn get_or_rasterize_glyph(
        &mut self,
        glyph_id: GlyphId,
        font_size_px: f32,
        queue: &wgpu::Queue,
        atlas_texture: &wgpu::Texture,
    ) -> CachedGlyph {
        let key_size = (font_size_px * 2.0).round().max(2.0) as u32;
        let key = (glyph_id, key_size);
        if let Some(cached) = self.cache.get(&key) {
            return *cached;
        }

        let scale = PxScale::from(font_size_px);
        let glyph = glyph_id.with_scale_and_position(scale, point(0.0, 0.0));
        let Some(outlined) = self.font.outline_glyph(glyph) else {
            let empty = CachedGlyph {
                uv_min: [0.0, 0.0],
                uv_max: [0.0, 0.0],
                offset_x: 0.0,
                offset_y: 0.0,
                width: 0.0,
                height: 0.0,
            };
            self.cache.insert(key, empty);
            return empty;
        };

        let bounds = outlined.px_bounds();
        let w = (bounds.width().ceil() as u32).max(1);
        let h = (bounds.height().ceil() as u32).max(1);

        if self.cursor_x + w + 1 >= ATLAS_SIZE {
            self.cursor_x = 1;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.cursor_y + h + 1 >= ATLAS_SIZE {
            // Atlas full: reset shelf packing cleanly.
            self.cache.clear();
            self.cursor_x = 1;
            self.cursor_y = 1;
            self.row_height = 0;
        }

        let atlas_x = self.cursor_x;
        let atlas_y = self.cursor_y;
        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);

        let mut bitmap = vec![0u8; (w * h) as usize];
        outlined.draw(|gx, gy, cov| {
            let idx = (gy * w + gx) as usize;
            if idx < bitmap.len() {
                bitmap[idx] = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: atlas_x,
                    y: atlas_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let inv = 1.0 / ATLAS_SIZE as f32;
        let cached = CachedGlyph {
            uv_min: [atlas_x as f32 * inv, atlas_y as f32 * inv],
            uv_max: [(atlas_x + w) as f32 * inv, (atlas_y + h) as f32 * inv],
            offset_x: bounds.min.x,
            offset_y: bounds.min.y,
            width: w as f32,
            height: h as f32,
        };
        self.cache.insert(key, cached);
        cached
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for FontSystem {
    fn measure_text(&mut self, text: &str, font_size: f32) -> Size {
        if text.is_empty() {
            return Size::new(0.0, font_size * 1.2);
        }
        let scaled = self.font.as_scaled(PxScale::from(font_size));
        let line_height = (scaled.ascent() - scaled.descent()).max(font_size);
        let mut max_w = 0.0_f32;
        let mut line_count = 0usize;

        for line in text.lines() {
            line_count += 1;
            let mut width = 0.0_f32;
            let mut prev: Option<GlyphId> = None;
            for ch in line.chars() {
                let gid = scaled.glyph_id(ch);
                if let Some(p) = prev {
                    width += scaled.kern(p, gid);
                }
                width += scaled.h_advance(gid);
                prev = Some(gid);
            }
            max_w = max_w.max(width);
        }

        let lines = line_count.max(1) as f32;
        Size::new(max_w.ceil(), (lines * line_height).ceil())
    }
}

/// WGPU batched SDF-quad and glyph-atlas renderer.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    index_buffer: wgpu::Buffer,
    index_capacity: usize,
    vertices: Vec<GpuVertex>,
    indices: Vec<u32>,
    pub font_system: FontSystem,
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rts_ui_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts_ui_globals"),
            size: std::mem::size_of::<GlobalsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rts_ui_glyph_atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rts_ui_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rts_ui_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rts_ui_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rts_ui_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rts_ui_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[GpuVertex::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let vertex_capacity = 4096;
        let index_capacity = 6144;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts_ui_vertices"),
            size: (vertex_capacity * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts_ui_indices"),
            size: (index_capacity * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
            atlas_texture,
            vertex_buffer,
            vertex_capacity,
            index_buffer,
            index_capacity,
            vertices: Vec::with_capacity(vertex_capacity),
            indices: Vec::with_capacity(index_capacity),
            font_system: FontSystem::new(),
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        viewport: Size,
        draw_list: &DrawList,
    ) {
        self.vertices.clear();
        self.indices.clear();

        // Layer 0: World-space overlays
        for q in &draw_list.world_quads {
            self.push_quad(q);
        }
        for t in &draw_list.world_texts {
            self.push_text(t, queue);
        }

        // Layer 1: HUD Chrome
        for q in &draw_list.hud_quads {
            self.push_quad(q);
        }
        for t in &draw_list.hud_texts {
            self.push_text(t, queue);
        }

        // Layer 2: Floating unclipped tooltips / overlays
        for q in &draw_list.overlay_quads {
            self.push_quad(q);
        }
        for t in &draw_list.overlay_texts {
            self.push_text(t, queue);
        }

        let globals = GlobalsUniform {
            screen_size: [viewport.width, viewport.height],
            _pad: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&globals));

        if !self.vertices.is_empty() && !self.indices.is_empty() {
            if self.vertices.len() > self.vertex_capacity {
                self.vertex_capacity = self.vertices.len().next_power_of_two();
                self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rts_ui_vertices"),
                    size: (self.vertex_capacity * std::mem::size_of::<GpuVertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            if self.indices.len() > self.index_capacity {
                self.index_capacity = self.indices.len().next_power_of_two();
                self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rts_ui_indices"),
                    size: (self.index_capacity * std::mem::size_of::<u32>()) as u64,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
            queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rts_ui_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.030,
                        g: 0.045,
                        b: 0.065,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if !self.indices.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
        }
    }

    fn push_quad(&mut self, q: &QuadPrimitive) {
        if q.rect.width <= 0.0 || q.rect.height <= 0.0 {
            return;
        }
        let base = self.vertices.len() as u32;
        let x0 = q.rect.x;
        let y0 = q.rect.y;
        let x1 = q.rect.x + q.rect.width;
        let y1 = q.rect.y + q.rect.height;
        let rect_size = [q.rect.width, q.rect.height];
        let bg = q.bg_color.to_array();
        let border = q.border_color.to_array();
        let params = [q.corner_radius, q.border_width, 0.0, 0.0];

        self.vertices.extend_from_slice(&[
            GpuVertex {
                pos: [x0, y0],
                uv: [0.0, 0.0],
                rect_size,
                bg_color: bg,
                border_color: border,
                params,
            },
            GpuVertex {
                pos: [x1, y0],
                uv: [q.rect.width, 0.0],
                rect_size,
                bg_color: bg,
                border_color: border,
                params,
            },
            GpuVertex {
                pos: [x1, y1],
                uv: [q.rect.width, q.rect.height],
                rect_size,
                bg_color: bg,
                border_color: border,
                params,
            },
            GpuVertex {
                pos: [x0, y1],
                uv: [0.0, q.rect.height],
                rect_size,
                bg_color: bg,
                border_color: border,
                params,
            },
        ]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_text(&mut self, run: &TextPrimitive, queue: &wgpu::Queue) {
        if run.text.is_empty() {
            return;
        }
        let font = self.font_system.font.clone();
        let scaled = font.as_scaled(PxScale::from(run.font_size));
        let ascent = scaled.ascent();
        let line_height = (scaled.ascent() - scaled.descent()).max(run.font_size);

        let mut pen_y = run.pos.y + ascent;
        let color = run.color.to_array();
        let border = [0.0, 0.0, 0.0, 0.0];
        let params = [0.0, 0.0, 1.0, 0.0];

        for line in run.text.lines() {
            let mut pen_x = run.pos.x;
            let mut prev: Option<GlyphId> = None;

            for ch in line.chars() {
                let gid = scaled.glyph_id(ch);
                if let Some(p) = prev {
                    pen_x += scaled.kern(p, gid);
                }
                let advance = scaled.h_advance(gid);
                prev = Some(gid);

                if ch.is_whitespace() {
                    pen_x += advance;
                    continue;
                }

                let cached = self.font_system.get_or_rasterize_glyph(
                    gid,
                    run.font_size,
                    queue,
                    &self.atlas_texture,
                );
                if cached.width > 0.0 && cached.height > 0.0 {
                    let gx0 = (pen_x + cached.offset_x).round();
                    let gy0 = (pen_y + cached.offset_y).round();
                    let gx1 = gx0 + cached.width;
                    let gy1 = gy0 + cached.height;
                    let rect_size = [cached.width, cached.height];
                    let base = self.vertices.len() as u32;

                    self.vertices.extend_from_slice(&[
                        GpuVertex {
                            pos: [gx0, gy0],
                            uv: [cached.uv_min[0], cached.uv_min[1]],
                            rect_size,
                            bg_color: color,
                            border_color: border,
                            params,
                        },
                        GpuVertex {
                            pos: [gx1, gy0],
                            uv: [cached.uv_max[0], cached.uv_min[1]],
                            rect_size,
                            bg_color: color,
                            border_color: border,
                            params,
                        },
                        GpuVertex {
                            pos: [gx1, gy1],
                            uv: [cached.uv_max[0], cached.uv_max[1]],
                            rect_size,
                            bg_color: color,
                            border_color: border,
                            params,
                        },
                        GpuVertex {
                            pos: [gx0, gy1],
                            uv: [cached.uv_min[0], cached.uv_max[1]],
                            rect_size,
                            bg_color: color,
                            border_color: border,
                            params,
                        },
                    ]);
                    self.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }

                pen_x += advance;
            }
            pen_y += line_height;
        }
    }
}
