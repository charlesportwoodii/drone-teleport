FROM ubuntu:20.04

LABEL maintainer="Charles R. Portwood II <charlesportwoodii@erianna.com" \
    org.label-schema.name="Drone Teleport" \
    org.label-schema.vendor="Charles R. Portwood II" \
    org.label-schema.schema-version="1.0"

ENV DEBIAN_FRONTEND=noninteractive

ENV PLUGIN_OP=connect

# Add the teleport user
RUN useradd -ms /bin/bash teleport

# Fix permissions on arm64
RUN ln -s /usr/bin/dpkg-split /usr/sbin/dpkg-split
RUN ln -s /usr/bin/dpkg-deb /usr/sbin/dpkg-deb
RUN ln -s /bin/rm /usr/sbin/rm
RUN ln -s /bin/tar /usr/sbin/tar

# Install dependencies
RUN apt update
RUN apt install curl ca-certificates -y --no-install-recommends

# Install Teleport
RUN curl https://apt.releases.teleport.dev/gpg -o /usr/share/keyrings/teleport-archive-keyring.asc
RUN echo "deb [signed-by=/usr/share/keyrings/teleport-archive-keyring.asc] https://deb.releases.teleport.dev/ stable main" | tee /etc/apt/sources.list.d/teleport.list > /dev/null
RUN apt update
RUN apt install teleport openssh-client -y --no-install-recommends
RUN apt-get clean
RUN apt-get autoclean
RUN rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*

# Copy all architectures into the container
ADD target/aarch64-unknown-linux-gnu/release/drone-teleport /usr/sbin/drone-teleport-aarch64
ADD target/x86_64-unknown-linux-gnu/release/drone-teleport /usr/sbin/drone-teleport-x86_64

# Copy the binary to the correct location and delete unwanted architectures
RUN mv /usr/sbin/drone-teleport-$(arch) /usr/sbin/drone-teleport
RUN rm /usr/sbin/drone-teleport-*

ENTRYPOINT /usr/sbin/drone-teleport $PLUGIN_OP