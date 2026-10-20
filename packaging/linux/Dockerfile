# Native builds happen here, not against the newer hosted runner's glibc.
FROM ubuntu:18.04@sha256:152dc042452c496007f07ca9127571cb9c29697f42acbfad72324b2bb2e43c98
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    build-essential pkg-config clang-9 libclang-9-dev libssl-dev \
    libfontconfig1-dev libfreetype6-dev fonts-dejavu-core libx11-dev libxcursor-dev \
    libxrandr-dev libxinerama-dev libxi-dev libgl1-mesa-dev \
    libxkbcommon-x11-dev libwayland-dev libasound2-dev libudev-dev \
    libxcb1-dev libxcb-shape0-dev libxcb-xfixes0-dev libjavascriptcoregtk-4.0-18 \
    python3.8 python3.8-venv python3-pip ca-certificates curl git patchelf binutils \
    && python3.8 -m venv /opt/python \
    && /opt/python/bin/python -m pip install --no-cache-dir pip==25.0.1 \
    && /opt/python/bin/python -m pip install --no-cache-dir cmake==3.31.6 \
    && rm -rf /var/lib/apt/lists/*
# New CMake runs on the old libc; updating it does not update the linker/sysroot.
ENV PATH=/opt/rust/bin:/opt/python/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    CARGO_HOME=/cargo CARGO_TARGET_DIR=/build CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
    CMAKE_BUILD_PARALLEL_LEVEL=2
WORKDIR /src
