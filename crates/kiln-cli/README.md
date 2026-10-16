# kiln-cli

The `kiln-cli` command line (installed as `kiln` by the Linux packages): convert, render, inspect, fidelity, benchmarks, the function list and the JSON control channel over stdio. This crate is a scaffold created by WP-50: it builds, is linted and tested in CI, and carries no engine logic yet; the work package that owns it in PLAN.md §11 fills it in.

Available now (WP-54): `kiln-cli render --scratch <file.txt> --png <out.png> [--width 1000] [--height 900]
[--zoom 1] [--dpr 1] [--scroll-x <px>] [--scroll-y 0] [--theme light|dark]` lays a text file out as a scratch
document and writes the frame the canvas surface would show, painted by `kiln-raster` (the page is centred
like the surface does unless `--scroll-x` is given).
