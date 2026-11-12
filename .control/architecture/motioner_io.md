# Dominio: motioner_io

## Proposito
(completar)

## Componentes clave
- Nombre — que hace, donde vive: `ruta/archivo:linea-linea`

## Diagrama
(agregar referencia a diagrams/motioner_io.mmd si aplica)

## Decisiones relevantes
- 

## Estado de documentacion
parcial — ultima verificacion: 2026-11-11


## Prompt 16 additions (imports and symbols)

- `svg::parse_svg` is a zero-dependency XML + SVG reader that produces a
  `motioner_core::Symbol` with flattened geometry and part anchors; the
  subset, the warnings and the arc tolerance are documented in D-0054:
  `crates/motioner_io/src/svg.rs:1-70`.
- `use_assets::ProjectAssets` implements the DSL `AssetResolver`: sandboxed
  path joining, SVG parsing and `.motion` source loading
  (`crates/motioner_io/src/use_assets.rs:1-120`).
- Line numbers of imported geometry are not meaningful after flattening;
  the CLI paths that compile programs call `run_with_assets` with this
  resolver: `crates/motioner_app/src/main.rs:1348-1355`.
