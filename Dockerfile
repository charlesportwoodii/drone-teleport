FROM charlesportwoodii/ubuntu:22.04-base

LABEL maintainer="Charles R. Portwood II <charlesportwoodii@erianna.com" \
    org.label-schema.name="Drone Teleport" \
    org.label-schema.vendor="Charles R. Portwood II" \
    org.label-schema.schema-version="1.0"

ENV DEBIAN_FRONTEND=noninteractive

ENV PLUGIN_OP=connect

# Add the teleport user
RUN useradd -ms /bin/bash Teleport

# Install dependencies
RUN apt update
RUN apt install curl ca-certificates -y --no-install-recommends

# Install Teleport
RUN curl https://apt.releases.teleport.dev/gpg -o /usr/share/keyrings/teleport-archive-keyring.asc
RUN echo "deb [signed-by=/usr/share/keyrings/teleport-archive-keyring.asc] https://deb.releases.teleport.dev/ stable main" | tee /etc/apt/sources.list.d/teleport.list > /dev/null
RUN apt update
RUN apt install teleport openssh-client drone-teleport -y --no-install-recommends
RUN apt-get clean
RUN apt-get autoclean
RUN rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*

ENTRYPOINT /usr/local/bin/drone-teleport $PLUGIN_OP
