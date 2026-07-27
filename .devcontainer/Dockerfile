FROM mcr.microsoft.com/devcontainers/base:bookworm

ARG DEBIAN_FRONTEND=noninteractive

# Tauri's Linux toolchain plus the native packaging and GIO backends used by
# LiScan. Keep Debian 12 as the baseline so generated binaries remain usable
# on distributions with older glibc versions.
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        appstream \
        build-essential \
        ca-certificates \
        curl \
        dbus-x11 \
        desktop-file-utils \
        dpkg-dev \
        fakeroot \
        file \
        fonts-dejavu-core \
        gvfs \
        gvfs-backends \
        libayatana-appindicator3-dev \
        libglib2.0-dev \
        libgtk-3-dev \
        libpolkit-gobject-1-dev \
        librsvg2-dev \
        libssl-dev \
        libwebkit2gtk-4.1-dev \
        libxdo-dev \
        patchelf \
        pkg-config \
        policykit-1 \
        rpm \
        wget \
        xdg-utils \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

ENV GDK_BACKEND=x11 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    WEBKIT_DISABLE_COMPOSITING_MODE=1
