# Linux build and test environment for Zuruf (Rust, embedded Chromium via CEF, UI build).
# Used where no native toolchain is available; the app itself targets Arch/Hyprland.
FROM rust:1-bookworm

# Optional: extra root certificates for networks with a TLS-inspecting proxy.
# Put PEM files into tools/dev/certs/ (not tracked by git); none are needed otherwise.
COPY certs/ /usr/local/share/ca-certificates/extra/
RUN update-ca-certificates
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt     NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt     CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt

RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake ninja-build clang pkg-config curl ca-certificates git xz-utils bzip2 \
        libgtk-3-dev libglib2.0-dev libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 \
        libcups2 libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
        libgbm1 libasound2 libpango-1.0-0 libcairo2 libx11-xcb1 libxshmfence1 libxss1 \
        libxtst6 libgl1 libegl1 fonts-dejavu-core \
        xvfb xauth x11-utils x11-apps imagemagick xdotool procps \
    && rm -rf /var/lib/apt/lists/*

# Node is only needed to build the UI bundle and to run browser tests.
COPY --from=node:24-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:24-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm     && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx     && npm install -g pnpm@11

RUN rustup component add clippy rustfmt
ENV CARGO_TARGET_DIR=/build/target CARGO_HOME=/build/cargo
WORKDIR /work
