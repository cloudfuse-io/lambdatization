#!/bin/sh

echo "Target distribution should be Ubuntu Focal 20.04"

if [ -z "$2" ]
  then
    echo "Provide runner address as first argument and Github token as second"
fi


REMOTE_ADDRESS=$1
GITHUB_TOKEN=$2
REMOTE_HOME="/home/ubuntu"
REMOTE_USER=ubuntu
REMOTE=$REMOTE_USER@$REMOTE_ADDRESS

ssh -o "StrictHostKeyChecking=no" $REMOTE 'bash -s' << ENDSSH
set -e

# install docker
sudo apt-get update
sudo apt-get install -y \
    ca-certificates \
    curl \
    gnupg \
	unzip \
	jq \
	python3-pip \
    lsb-release
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
sudo add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu focal stable"
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
sudo usermod -aG docker \$USER


mkdir actions-runner && cd actions-runner
curl -o actions-runner-linux-x64-2.299.1.tar.gz -L https://github.com/actions/runner/releases/download/v2.299.1/actions-runner-linux-x64-2.299.1.tar.gz
tar xzf ./actions-runner-linux-x64-2.299.1.tar.gz
./config.sh --url https://github.com/cloudfuse-io/lambdatization --token $GITHUB_TOKEN --unattended --name github-action-runner-1-renamed
mkdir /home/ubuntu/hostedtoolcache
sudo ./svc.sh install
sudo ./svc.sh start
ENDSSH
