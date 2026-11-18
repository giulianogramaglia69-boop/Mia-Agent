//! Effetto WGSL per il canvas del grafo (Fase 5).
//!
//! Il canvas disegna nodi e archi col `Painter` 2D in un target
//! offscreen; questo shader ci passa sopra e aggiunge un bagliore
//! sottile color accento, proporzionale alla luminosità del pixel.
//! È un effetto a frammenti, non geometria: i nodi restano disegnati
//! dal `Painter` (un nodo procedurale nello shader è bloccato dal
//! limite di 256 byte per le uniform).
//!
//! Lo shader è validato da naga a compile time; va registrato con
//! `App::shader_type::<GraphGlow>()` o il disegno viene saltato in
//! silenzio.

use sequoia_shader::shader;

// Bagliore accento per il grafo: `strength` 0.0–1.0, `tint` il colore.
shader! {
    pub fn graph_glow(uv: FragUv, content: Sampler2D, strength: f32, tint: Color) -> Color {
        let c = textureSample(content, content_sampler, uv);
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let glow = tint.rgb * lum * strength;
        return vec4<f32>(c.rgb + glow * c.a, c.a);
    }
}

/// L'istanza usata dal canvas del grafo (accento #0A84FF, forza 0.35).
pub fn graph_glow() -> GraphGlow {
    GraphGlow {
        strength: 0.35,
        tint: sequoia_shader::Color::rgb(0.04, 0.52, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use sequoia_shader::SequoiShader;

    use super::*;

    // Verifica a compile time: lo shader non deve superare il limite
    // di 256 byte per le uniform.
    const _: () = assert!(GraphGlow::UNIFORM_SIZE <= 256);

    #[test]
    fn shader_name_matches() {
        assert_eq!(GraphGlow::NAME, "graph_glow");
    }

    #[test]
    fn default_instance_is_sane() {
        let glow = graph_glow();
        assert!((glow.strength - 0.35).abs() < f32::EPSILON);
        assert!((glow.tint.r - 0.04).abs() < 0.01);
    }
}
