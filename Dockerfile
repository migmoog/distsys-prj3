FROM rust:1.83 AS builder

# Set the working directory inside the container
WORKDIR /usr/src/app

# Copy the Cargo.toml and Cargo.lock (if available)
COPY Cargo.toml Cargo.lock ./

# Create a dummy src/main.rs file for dependency caching
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Pre-fetch dependencies to cache them
RUN cargo update
RUN cargo build --release && rm -rf target/release/build

# Copy the actual source code
COPY src/ src/

# Build the actual application
RUN cargo build --release

# ---- Runtime Stage ----
FROM ubuntu:22.04

# Install only the required shared libraries
RUN apt-get update && apt-get install -y \
    libssl-dev \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app
ADD hostsfile.txt /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/app/target/release/prj3 .

# Run the application by default
ENTRYPOINT ["./prj3"]
