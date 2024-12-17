variable "image_name" {
  type    = string
  default = "feature-server"
}
variable "image_tag" {
  type    = string
  default = "0.0.5"
}

resource "aws_ecr_repository" "ecr" {
  name         = "feature_server"
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
    docker build -t ${var.image_name}:${var.image_tag} ../ -f ../dockerfiles/Dockerfile --build-arg APP_NAME=feature_server
    docker tag ${var.image_name}:${var.image_tag} ${aws_ecr_repository.ecr.repository_url}:${var.image_tag}
    docker push ${aws_ecr_repository.ecr.repository_url}:${var.image_tag}
    EOT
  }
  triggers = {
    ecr_repo   = aws_ecr_repository.ecr.id
    image_name = var.image_name
    image_tag  = var.image_tag
  }
}

resource "null_resource" "upload_image_feats_update" {
  provisioner "local-exec" {
    command = <<EOT
    aws ecr get-login-password --region ${var.region} | docker login --username AWS --password-stdin ${aws_ecr_repository.ecr_update_features.repository_url}
    docker build -t ${var.image_name}:${var.image_tag} ../ -f ../dockerfiles/Dockerfile --build-arg APP_NAME=update_features
    docker tag ${var.image_name}:${var.image_tag} ${aws_ecr_repository.ecr_update_features.repository_url}:${var.image_tag}
    docker push ${aws_ecr_repository.ecr_update_features.repository_url}:${var.image_tag}
    EOT
  }
  triggers = {
    ecr_repo   = aws_ecr_repository.ecr.id
    image_name = var.image_name
    image_tag  = var.image_tag
  }
}
