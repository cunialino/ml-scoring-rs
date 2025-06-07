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
  helm upgrade --install ngix-ingress ngix-ingress/ingress-nginx -f k8s/chartsValues/ngix.yaml --namespace network --create-namespace $EXTRA_ARG_NGINX
  helm upgrade --install kube-metrics kube-metrics/metrics-server -f k8s/chartsValues/kube-metrics.yaml -n kube-system --set global.security.allowInsecureImages=true $EXTRA_ARG

local-deploy:
  kind export kubeconfig --name ml-scoring
  just --justfile {{justfile()}} install-charts
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
