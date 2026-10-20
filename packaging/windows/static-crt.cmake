# Match Rust's -C target-feature=+crt-static for raylib and its GLFW subproject.
# CMP0091 makes the runtime a target property rather than only a /MT compiler flag.
set(CMAKE_POLICY_DEFAULT_CMP0091 NEW CACHE STRING "" FORCE)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>" CACHE STRING "" FORCE)
set(USE_MSVC_RUNTIME_LIBRARY_DLL OFF CACHE BOOL "" FORCE)
