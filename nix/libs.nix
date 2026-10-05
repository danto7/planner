# Libraries winit/glutin load with dlopen at runtime. They are not linked at
# build time, so they must be on LD_LIBRARY_PATH when the binary runs, or you
# get errors such as `WaylandError(Connection(NoWaylandLib))`.
{ pkgs }:
let
  # nixpkgs renamed the xorg.* attributes to top-level names in late 2026;
  # fall back to the old names so older nixpkgs revisions keep working.
  x = new: old: pkgs.${new} or pkgs.xorg.${old};
in
[
  pkgs.wayland
  pkgs.libxkbcommon
  pkgs.libGL
  (x "libx11" "libX11")
  (x "libxcursor" "libXcursor")
  (x "libxi" "libXi")
  (x "libxrandr" "libXrandr")
  (x "libxcb" "libxcb")
]
