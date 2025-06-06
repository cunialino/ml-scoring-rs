variable "image_name" {
  type    = string
  default = "scoring-server"
}
variable "scoring_tag" {
  type    = string
  default = "0.0.13"
}

variable "updates_tag" {
  type    = string
  default = "0.0.1"
}

resource "aws_ecr_repository" "ecr" {
  name         = "scoring_server"
  force_delete = true
}

resource "aws_ecr_repository" "ecr_update_features" {
  name         = "update_features"
  force_delete = true
}

resource "null_resource" "upload_image" {
  provisioner "local-exec" {
    command = <<EOT
    aws ecr get-login-password --region ${var.region} | docker login --username AWS --password-stdin ${aws_ecr_repository.ecr.repository_url}
    docker build -t ${var.image_name}:${var.scoring_tag} ../ -f ../dockerfiles/Dockerfile --build-arg APP_NAME=scoring_server
    docker tag ${var.image_name}:${var.scoring_tag} ${aws_ecr_repository.ecr.repository_url}:${var.scoring_tag}
    docker push ${aws_ecr_repository.ecr.repository_url}:${var.scoring_tag}
    EOT
  }
  triggers = {
    ecr_repo   = aws_ecr_repository.ecr.id
    image_name = var.image_name
    scoring_tag  = var.scoring_tag
  }
}

resource "null_resource" "upload_image_feats_update" {
  provisioner "local-exec" {
    command = <<EOT
    aws ecr get-login-password --region ${var.region} | docker login --username AWS --password-stdin ${aws_ecr_repository.ecr_update_features.repository_url}
    docker build -t ${var.image_name}:${var.updates_tag} ../ -f ../dockerfiles/Dockerfile --build-arg APP_NAME=update_features
    docker tag ${var.image_name}:${var.updates_tag} ${aws_ecr_repository.ecr_update_features.repository_url}:${var.updates_tag}
    docker push ${aws_ecr_repository.ecr_update_features.repository_url}:${var.updates_tag}
    EOT
  }
  triggers = {
    ecr_repo   = aws_ecr_repository.ecr.id
    image_name = var.image_name
    updates_tag  = var.updates_tag
  }
}


locals {
  images = {
    "quay.io/prometheus/prometheus"                          = "v3.4.1"
    "docker.io/grafana/grafana"                              = "12.0.0-security-01"
    "docker.io/bitnami/kube-state-metrics"                   = "2.15.0-debian-12-r12"
    "registry.k8s.io/ingress-nginx/kube-webhook-certgen"     = "v1.5.3"
    "quay.io/prometheus-operator/prometheus-operator"        = "v0.82.2"
    "docker.io/kiwigrid/k8s-sidecar"                         = "1.30.0"
    "quay.io/prometheus-operator/prometheus-config-reloader" = "v0.82.2"
    "registry.k8s.io/ingress-nginx/controller"               = "v1.12.2"
    "docker.io/bitnami/metrics-server"                       = "0.7.2-debian-12-r24"
    "public.ecr.aws/eks/aws-load-balancer-controller"        = "v2.13.2"
  }
}

locals {
  repo_names = {
    for full, tag in local.images :
    full => join(
      "/",
      slice(
        split("/", full),
        1,
        length(split("/", full))
      )
    )
  }
}

# Create an ECR repo for each image, named like "prometheus/prometheus", "grafana/grafana", etc.
resource "aws_ecr_repository" "images" {
  for_each = local.images

  name         = local.repo_names[each.key]
  force_delete = true
}

# Mirror upstream → your ECR
resource "null_resource" "mirror" {
  for_each = local.images

  triggers = {
    # re-run when the tag changes
    image_tag = each.value
  }

  provisioner "local-exec" {
    command = <<-EOT
      docker pull ${each.key}:${each.value}
      docker tag ${each.key}:${each.value} \
        ${aws_ecr_repository.images[each.key].repository_url}:${each.value}
      aws ecr get-login-password --region ${var.region} \
        | docker login --username AWS --password-stdin \
          ${aws_ecr_repository.images[each.key].repository_url}
      docker push ${aws_ecr_repository.images[each.key].repository_url}:${each.value}
    EOT
  }
}

locals {
  scoring_patch_yaml = templatefile("${path.module}/scoring-patch.yaml", {
    repo = aws_ecr_repository.ecr.repository_url
    tag = var.scoring_tag
  })
}

resource "local_file" "ecr_scoring_patch" {
  filename = "${path.module}/../k8s/overlays/aws/patches/scoring-patch.yaml"
  content  = local.scoring_patch_yaml
}

locals {
  update_patch_yaml = templatefile("${path.module}/updates-patch.yaml", {
    repo = aws_ecr_repository.ecr_update_features.repository_url
    tag = var.updates_tag
  })
}

resource "local_file" "ecr_update_patch" {
  filename = "${path.module}/../k8s/overlays/aws/patches/updates-patch.yaml"
  content  = local.update_patch_yaml
}
