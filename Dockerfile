### Build image
FROM debian:buster-slim AS sora-node-build

# Set environment variables
ENV RUSTUP_HOME="/opt/rust"
ENV CARGO_HOME="/opt/rust"
ENV PATH="$PATH:$RUSTUP_HOME/bin"
ENV RUST_VERSION=nightly-2021-03-11

# Install dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
        ca-certificates apt-transport-https gnupg \
        libssl-dev pkg-config \
        curl \
        git \
        software-properties-common \
        llvm clang && \
    rm -rf /var/lib/apt/lists/*

# Install docker
RUN curl -fsSL https://download.docker.com/linux/ubuntu/gpg | apt-key add -
RUN add-apt-repository \
        "deb https://download.docker.com/linux/debian $(lsb_release -cs) stable" && \
    apt-get update && \
    apt-get install --no-install-recommends -y \
        docker-ce docker-ce-cli containerd.io && \
    rm -rf /var/lib/apt/lists/*

# Install rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain ${RUST_VERSION} && \
    rustup target add wasm32-unknown-unknown

RUN cargo install sccache
ENV RUSTC_WRAPPER=sccache
ENV CARGO_INCREMENTAL=0

# Build project
ADD . /build
WORKDIR /build
RUN cargo build --release --features include-real-files
RUN cp target/release/framenode housekeeping/framenode
# RUN cargo run --bin framenode --release --features include-real-files -- build-spec --chain main-coded --raw > node/chain_spec/src/bytes/chain_spec_main.json
# RUN cargo test --release

### Node image
FROM debian:buster-slim AS sora-node

# Install dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
        npm ca-certificates nano curl && \
	apt-get autoremove -y && \
	apt-get clean && \
	find /var/lib/apt/lists/ -type f -not -name lock -delete

# Install @polkadot/api-cli
ENV POLKADOT_API_PATH=/usr/local/lib/node_modules/@polkadot
RUN npm install -g @polkadot/api-cli@0.22.1
RUN rm -rf $POLKADOT_API_PATH/api-cli
COPY ./housekeeping/docker/release/api-cli.tar.gz $POLKADOT_API_PATH/api-cli.tar.gz
RUN tar -xzf $POLKADOT_API_PATH/api-cli.tar.gz -C $POLKADOT_API_PATH/

# Node config
RUN useradd substrate -u 10000
COPY --from=sora-node-build ./build/target/release/framenode /usr/local/bin/framenode
COPY --chown=10000:10000 ./housekeeping/docker/release/parachain_registration.sh /opt/sora2/parachain_registration.sh
RUN chmod +x /opt/sora2/parachain_registration.sh  && \
    mkdir /chain && \
    chown 10000:10000 /chain
COPY --from=sora-node-build --chown=10000:10000 ./build/node/chain_spec/src/bytes/chain_spec_main.json /chain.spec

USER substrate
ENTRYPOINT ["framenode"]
