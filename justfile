# Define variables for reusability
TERRAFORM_DIR := "terraform"
AWS_REGION := "eu-west-1" # Adjust to your desired AWS region
IMAGE_NAME := "feature-server" # Replace with your Docker image name
TAG := "latest" # Replace with the desired tag

KIND_NAME := "ml-scoring"

# Default task
default:
    @echo "Run 'just apply-terraform' or 'just upload-ecr-image'"

# Task to apply Terraform infrastructure
apply-terraform:
    @echo "Applying Terraform infrastructure in {{TERRAFORM_DIR}}"
    cd {{TERRAFORM_DIR}} && terraform init && terraform apply -var="my_ip=$(curl ipinfo.io/ip)" -var="cidr_block=32" -var="docker_token=$(cat ~/dockersecrettoken)"

destroy-terraform:
  @echo "Destroying Terraform infrastructure"
  cd {{TERRAFORM_DIR}} && terraform destroy -var="my_ip=$(curl ipinfo.io/ip)" -var="cidr_block=32"

start-cluster:
  kind create cluster -n {{KIND_NAME}} --config k8s/kind-config.yaml
  just --justfile {{justfile()}} build-images

install-charts env="local":
  #!/bin/env sh
  if [ "{{env}}" = "aws" ]; then 
    EXTRA_ARG_NGINX="--set global.image.registry=790283579703.dkr.ecr.eu-west-1.amazonaws.com   --set controller.image.digest="
    EXTRA_ARG="--set global.imageRegistry=790283579703.dkr.ecr.eu-west-1.amazonaws.com"
  else
    echo "nothing"
  fi
  helm upgrade --wait --install prometheus prometheus-community/kube-prometheus-stack -f k8s/chartsValues/prometheus-values.yaml --namespace monitoring --create-namespace $EXTRA_ARG
  helm upgrade --install --create-namespace contour bitnami/contour  --namespace projectcontour -f k8s/chartsValues/contour.yaml $EXTRA_ARG
  helm upgrade --install kube-metrics kube-metrics/metrics-server -f k8s/chartsValues/kube-metrics.yaml -n kube-system --set global.security.allowInsecureImages=true $EXTRA_ARG

local-deploy:
  kind export kubeconfig --name ml-scoring
  just --justfile {{justfile()}} install-charts
  just --justfile {{justfile()}} generate-metallb-manifest
  helm upgrade --wait --install metallb metallb/metallb --namespace metallb-system --create-namespace
  kubectl apply -k k8s/overlays/dev

aws-deploy:
  aws eks update-kubeconfig --name ml-scoring
  just --justfile {{justfile()}} install-charts aws
  kubectl apply -k k8s/overlays/aws


build-images:
  docker build . -f dockerfiles/Dockerfile --build-arg APP_NAME=scoring_server -t ml-scoring-server:dev
  docker build . -f dockerfiles/Dockerfile --build-arg APP_NAME=update_features -t ml-update-features:dev
  kind load docker-image ml-scoring-server:dev -n {{KIND_NAME}}
  kind load docker-image ml-update-features:dev -n {{KIND_NAME}}

delete-cluster:
  kubectl delete all --all --all-namespaces
  kind delete cluster -n {{KIND_NAME}}

restart-cluster:
  just --justfile {{justfile()}} delete-cluster
  just --justfile {{justfile()}} start-cluster

stress-test rate="100" duration="30s":
  #!/usr/bin/env bash
  set -u

  NAMESPACE="projectcontour"
  SERVICE_NAME="contour-envoy" # Corrected service name based on your example

  VEGA_RATE={{rate}}
  VEGA_DURATION={{duration}}
  LB_HOST=$(kubectl get svc -n network ngix-ingress-ingress-nginx-controller -o jsonpath='{.status.loadBalancer.ingress[0].hostname}')
  IP_ADDRESS="k8s-kind.com"
  if [[ -n "$LB_HOST" ]]; then
    TARGET_URL="http://$LB_HOST/score"
  else
    TARGET_URL="http://$IP_ADDRESS/score"
  fi

  NUM_TARGETS=$(echo "$VEGA_RATE * $(echo "$VEGA_DURATION" | sed 's/s//') * 1.1" | bc | cut -d'.' -f1) # 10% buffer
  TARGETS_FILE="vegeta_targets_$(date +%s%N).json"

  echo "Starting Vegeta attack from $TARGETS_FILE..."

  cargo run --release -p requests-generator -- $TARGET_URL \
  | vegeta attack \
        -lazy \
        -format=json \
        -rate=$VEGA_RATE \
        -duration=$VEGA_DURATION \
        -connections=0 \
        -workers=0 \
        -timeout=5s \
  | vegeta report

generate-metallb-manifest:
  #!/bin/bash

  OUTPUT_PATH="./k8s/overlays/dev/metallb.yaml"

  mkdir -p "$(dirname "$OUTPUT_PATH")"

  IPV4_SUBNET=$(docker inspect kind | jq -r '.[].IPAM.Config[] | select(.Subnet | contains(":_") | not) | .Subnet' | head -n 1)
  IPV4_GATEWAY=$(docker inspect kind | jq -r '.[].IPAM.Config[] | select(.Subnet | contains(":_") | not) | .Gateway' | head -n 1)

  if [[ -z "$IPV4_SUBNET" || -z "$IPV4_GATEWAY" ]]; then
      echo "Error: Could not determine IPv4 Subnet and/or Gateway from 'docker inspect kind'." >&2
      exit 1
  fi

  NETWORK_PREFIX=$(echo "$IPV4_GATEWAY" | awk -F'.' '{print $1"."$2"."$3}')

  IP_POOL_START="$NETWORK_PREFIX.0"
  IP_POOL_END="$NETWORK_PREFIX.24"
  IP_ADDRESS_RANGE="${IP_POOL_START}-${IP_POOL_END}"

  echo "Detected IPv4 Subnet: $IPV4_SUBNET"
  echo "Detected IPv4 Gateway: $IPV4_GATEWAY"
  echo "Generated MetalLB IP Address Pool Range: $IP_ADDRESS_RANGE"
  echo "Saving MetalLB configuration to: $OUTPUT_PATH"
  echo ""

  cat <<EOF > "$OUTPUT_PATH"
  apiVersion: metallb.io/v1beta1
  kind: IPAddressPool
  metadata:
    name: default
    namespace: metallb-system
  spec:
    addresses:
      - ${IP_ADDRESS_RANGE}
  ---
  apiVersion: metallb.io/v1beta1
  kind: L2Advertisement
  metadata:
    name: default
    namespace: metallb-system
  EOF

  echo "MetalLB configuration successfully saved to $OUTPUT_PATH"

add-charts:
  helm repo add ngix-ingress https://kubernetes.github.io/ingress-nginx
  helm repo add metallb https://metallb.github.io/metallb
  helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
  helm repo add kube-metrics https://charts.bitnami.com/bitnami

