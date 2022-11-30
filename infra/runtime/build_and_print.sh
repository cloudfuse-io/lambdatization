#!/bin/bash

COMPOSE_FILE=../../../docker/$1/docker-compose.yaml
l12n docker-login \
    build-images --compose-file=$COMPOSE_FILE \
    push-images --compose-file=$COMPOSE_FILE && \
l12n print-image-vars --compose-file=$COMPOSE_FILE > images.generated.tfvars
