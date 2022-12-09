FROM python:3.10.3-slim as cli-base

ARG TERRAFORM_VERSION=1.3.6
ARG TERRAGRUNT_VERSION=0.42.3

# Install base tools
RUN apt-get update \
    && apt-get install -y \
        gnupg \
        software-properties-common \
        curl \
        unzip \
        git \
        wget \
        expect \
        bindfs \
        sudo

# Install Terraform
RUN curl -fsSL https://apt.releases.hashicorp.com/gpg | apt-key add - \
    && apt-add-repository "deb [arch=amd64] https://apt.releases.hashicorp.com $(lsb_release -cs) main" \
    && apt-get update \
    && apt-get install terraform=$TERRAFORM_VERSION

RUN curl -L https://github.com/gruntwork-io/terragrunt/releases/download/v${TERRAGRUNT_VERSION}/terragrunt_linux_amd64 -o /usr/local/bin/terragrunt \
    && chmod +x /usr/local/bin/terragrunt

# Install Docker
RUN curl -fsSL https://download.docker.com/linux/$(. /etc/os-release; echo "$ID")/gpg | apt-key add - \
    && add-apt-repository \
    "deb [arch=amd64] https://download.docker.com/linux/$(. /etc/os-release; echo "$ID") \
    $(lsb_release -cs) \
    stable" \
    && apt-get update \
    && apt-get -y install docker-ce docker-compose-plugin

# Install AWS Session Manager Plugin
RUN curl "https://s3.amazonaws.com/session-manager-downloads/plugin/latest/ubuntu_64bit/session-manager-plugin.deb" -o "session-manager-plugin.deb" \
    && dpkg -i session-manager-plugin.deb

# Install AWS CLI
RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" \
    && unzip awscliv2.zip \
    && ./aws/install

# Install GCP CLI
RUN echo "deb [signed-by=/usr/share/keyrings/cloud.google.gpg] http://packages.cloud.google.com/apt cloud-sdk main" \
    | tee -a /etc/apt/sources.list.d/google-cloud-sdk.list \
    && wget -O - "https://packages.cloud.google.com/apt/doc/apt-key.gpg" \
    | apt-key --keyring /usr/share/keyrings/cloud.google.gpg add - \
    && apt-get update \
    && apt-get -y --no-install-recommends install \
        python3-crcmod \
        google-cloud-sdk

ARG UNAME=cli

# Configure the host caller user/group and host docker group in the image
RUN groupadd cli \  
    && useradd --create-home -g cli $UNAME

ENV CONTAINER_USER=cli
ENV CONTAINER_GROUP=cli

# Setup persistent folders for configs
RUN owneddir() { mkdir -p $1 && chown $UNAME:$UNAME $1 ; } \
    && owneddir /etc/persistent-configs \
    && owneddir /etc/persistent-configs/gcloud \
    && owneddir /etc/persistent-configs/docker


RUN mkdir /home/$UNAME/.config \
    && touch /etc/persistent-configs/bash_history \
    && ln -s /etc/persistent-configs/docker /home/$UNAME/.docker \
    && ln -s /etc/persistent-configs/gcloud /home/$UNAME/.config/gcloud \
    && ln -s /etc/persistent-configs/bash_history /home/$UNAME/.bash_history

# Install Python dependencies
COPY cli/requirements.txt /requirements.txt
RUN pip install -r /requirements.txt

FROM cli-base as lambda

COPY . /repo
ENV REPO_DIR=/repo
ENV CALLING_DIR=/tmp/
ENV L12N_PLUGINS=lambdacli

ENTRYPOINT [ "/repo/l12n", "lambdacli.run-bootstrap" ]


FROM cli-base as cli

COPY docker/cli/entrypoint.sh /entrypoint.sh

ENTRYPOINT [ "/entrypoint.sh" ]
