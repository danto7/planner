# Libraries winit/glutin load with dlopen at runtime. They are not linked at
# build time, so they must be on LD_LIBRARY_PATH when the binary runs, or you
# get errors such as `WaylandError(Connection(NoWaylandLib))`.
{ wayland, libxkbcommon, libGL, xorg }:
[
  wayland
  libxkbcommon
  libGL
  xorg.libX11
  xorg.libXcursor
  xorg.libXi
  xorg.libXrandr
  xorg.libxcb
]
