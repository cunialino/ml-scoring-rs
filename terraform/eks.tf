variable "cluster_name" {
  type    = string
  default = "ml-scoring"
}

variable "cluster_version" {
  type    = string
  default = "1.33"
}

resource "aws_iam_role" "eks_cluster" {
  name = var.cluster_name

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Sid    = ""
        Principal = {
          Service = "eks.amazonaws.com"
        }
      },
    ]
  })
}

resource "aws_iam_policy_attachment" "eks-cluster-policy" {
  name       = "eks-cluster-policy"
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSClusterPolicy"
  roles      = [aws_iam_role.eks_cluster.name]
}

resource "aws_eks_cluster" "ml-scoring" {
  name     = var.cluster_name
  version  = var.cluster_version
  role_arn = aws_iam_role.eks_cluster.arn
  vpc_config {
    endpoint_private_access = true
    endpoint_public_access  = true
    public_access_cidrs     = ["0.0.0.0/0"]
    subnet_ids              = concat(aws_subnet.private_subnets.*.id, aws_subnet.public_subnets.*.id, )
  }

  depends_on = [aws_iam_policy_attachment.eks-cluster-policy]

}

resource "aws_iam_role" "eks-fargate-profile" {
  name = "fargate-profile-${var.cluster_name}"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Sid    = ""
        Principal = {
          Service = "eks-fargate-pods.amazonaws.com"
        }
      },
    ]
  })
}

resource "aws_iam_policy_attachment" "eks-fargate-policy" {
  name       = "eks-fargate-policy"
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSFargatePodExecutionRolePolicy"
  roles      = [aws_iam_role.eks-fargate-profile.name]
}

resource "aws_eks_fargate_profile" "kube-system" {
  cluster_name           = aws_eks_cluster.ml-scoring.name
  fargate_profile_name   = "kube-system"
  pod_execution_role_arn = aws_iam_role.eks-fargate-profile.arn
  subnet_ids             = aws_subnet.private_subnets.*.id
  selector {
    namespace = "kube-system"
  }
}

resource "aws_eks_fargate_profile" "monitoring" {
  cluster_name           = aws_eks_cluster.ml-scoring.name
  fargate_profile_name   = "monitoring"
  pod_execution_role_arn = aws_iam_role.eks-fargate-profile.arn
  subnet_ids             = aws_subnet.private_subnets.*.id
  selector {
    namespace = "monitoring"
  }
}

resource "aws_eks_fargate_profile" "network" {
  cluster_name           = aws_eks_cluster.ml-scoring.name
  fargate_profile_name   = "network"
  pod_execution_role_arn = aws_iam_role.eks-fargate-profile.arn
  subnet_ids             = aws_subnet.private_subnets.*.id
  selector {
    namespace = "network"
  }
}

resource "aws_eks_fargate_profile" "scoring" {
  cluster_name           = aws_eks_cluster.ml-scoring.name
  fargate_profile_name   = "scoring"
  pod_execution_role_arn = aws_iam_role.eks-fargate-profile.arn
  subnet_ids             = aws_subnet.private_subnets.*.id
  selector {
    namespace = "scoring"
  }
}
data "aws_region" "current" {}

resource "null_resource" "restart_coredns" {
  depends_on = [
    aws_eks_fargate_profile.kube-system
  ]

  provisioner "local-exec" {
    command = <<EOT
    # Update kubeconfig (this example assumes AWS CLI v2 and default AWS profile)
    aws eks update-kubeconfig --name ${aws_eks_cluster.ml-scoring.name} --region ${data.aws_region.current.name}
    
    # Restart CoreDNS deployment
    kubectl rollout restart -n kube-system deployment/coredns
    EOT
  }
}

resource "aws_eks_addon" "vpc-cni" {
  cluster_name                = aws_eks_cluster.ml-scoring.name
  addon_name                  = "vpc-cni"
  resolve_conflicts_on_update = "OVERWRITE"
}
resource "aws_eks_addon" "coredns" {
  cluster_name                = aws_eks_cluster.ml-scoring.name
  addon_name                  = "coredns"
  resolve_conflicts_on_update = "OVERWRITE"
  depends_on                  = [aws_eks_fargate_profile.kube-system, null_resource.restart_coredns]
}
resource "aws_eks_addon" "kube-proxy" {
  cluster_name                = aws_eks_cluster.ml-scoring.name
  addon_name                  = "kube-proxy"
  resolve_conflicts_on_update = "OVERWRITE"
}

data "tls_certificate" "eks_oidc" {
  # Point at the full issuer URL (including "https://…")
  url = aws_eks_cluster.ml-scoring.identity[0].oidc[0].issuer
}

resource "aws_iam_openid_connect_provider" "eks" {
  # Must be the exact HTTPS issuer URL, without a trailing slash
  url = aws_eks_cluster.ml-scoring.identity[0].oidc[0].issuer

  # sts.amazonaws.com is needed for IRSA
  client_id_list = ["sts.amazonaws.com"]

  # Use the TLS data source to grab the SHA1 thumbprint
  thumbprint_list = [
    data.tls_certificate.eks_oidc.certificates[0].sha1_fingerprint
  ]

  # (Optional) You can tag it or add lifecycle/depends_on if you like.
  depends_on = [aws_eks_cluster.ml-scoring]
}

data "aws_iam_policy_document" "aws-lb-ctrk-arp" {
  statement {
    actions = ["sts:AssumeRoleWithWebIdentity"]
    effect  = "Allow"

    condition {
      test     = "StringEquals"
      variable = "${replace(aws_iam_openid_connect_provider.eks.url, "https://", "")}:sub"
      values   = ["system:serviceaccount:kube-system:aws-load-balancer-controller"]
    }

    principals {
      identifiers = [aws_iam_openid_connect_provider.eks.arn]
      type        = "Federated"
    }
  }
}

resource "aws_iam_role" "aws_load_balancer_controller" {
  assume_role_policy = data.aws_iam_policy_document.aws-lb-ctrk-arp.json
  name               = "aws-load-balancer-controller"
}

resource "aws_iam_policy" "aws_load_balancer_controller" {
  policy = file("./policy.json")
  name   = "AwsLoadBalancerController"
}

resource "aws_iam_policy_attachment" "aws_load_balancer_controller_attach" {
  name       = "lb-attach"
  roles      = [aws_iam_role.aws_load_balancer_controller.name]
  policy_arn = aws_iam_policy.aws_load_balancer_controller.arn
}

data "aws_eks_cluster" "cluster" {
  name = aws_eks_cluster.ml-scoring.name
}

data "aws_eks_cluster_auth" "cluster" {
  name = aws_eks_cluster.ml-scoring.name
}
provider "kubernetes" {
  host                   = data.aws_eks_cluster.cluster.endpoint
  cluster_ca_certificate = base64decode(data.aws_eks_cluster.cluster.certificate_authority[0].data)
  token                  = data.aws_eks_cluster_auth.cluster.token
  # (Optional) You can specify load_config_file = false if you don't want to merge with ~/.kube/config
}

# -------------------------------------------------------------------
# 3c. Helm provider (uses the same Kubernetes connection)
# -------------------------------------------------------------------
resource "kubernetes_service_account" "aws_load_balancer_controller" {
  metadata {
    name      = "aws-load-balancer-controller"
    namespace = "kube-system"

    annotations = {
      # This annotation “glues” the ServiceAccount to the IAM role via OIDC
      "eks.amazonaws.com/role-arn" = aws_iam_role.aws_load_balancer_controller.arn
    }
  }
}

provider "helm" {
  kubernetes {
    host                   = data.aws_eks_cluster.cluster.endpoint
    cluster_ca_certificate = base64decode(data.aws_eks_cluster.cluster.certificate_authority[0].data)
    token                  = data.aws_eks_cluster_auth.cluster.token
  }
}

resource "helm_release" "aws_load_balancer_controller" {
  name       = "aws-load-balancer-controller"
  repository = "https://aws.github.io/eks-charts"
  chart      = "aws-load-balancer-controller"
  version    = "1.13.2" # pick a chart version that matches your EKS version. (1.9.3 was current as of mid-2025.)
  namespace  = "kube-system"

  # Tell the chart not to create its own ServiceAccount

  set {
    name  = "serviceAccount.name"
    value = kubernetes_service_account.aws_load_balancer_controller.metadata[0].name
  }
  set {
    name  = "serviceAccount.create"
    value = "false"
  }

  # Point at your cluster by name
  set {
    name  = "clusterName"
    value = aws_eks_cluster.ml-scoring.id
  }
  set {
    name  = "image.repository"
    value = "790283579703.dkr.ecr.eu-west-1.amazonaws.com/eks/aws-load-balancer-controller"
  }


  # (Optional) If your cluster is in a non-default region, uncomment this:
  set {
    name  = "region"
    value = "eu-west-1"
  }

  set {
    name  = "vpcId"
    value = aws_vpc.scoring_vpc.id
  }

  depends_on = [
    aws_eks_fargate_profile.kube-system
  ]
}

locals {
  efs_patch_yaml = templatefile("${path.module}/aws_kustomize_patches.yaml", {
    efs_id = aws_efs_file_system.my_efs.id
  })
}

resource "local_file" "efs_patch" {
  filename = "${path.module}/../k8s/overlays/aws/patches/rocksdb-pv.yaml"
  content  = local.efs_patch_yaml
}

output "aws_lbc_role_arn" {
  value = aws_iam_role.aws_load_balancer_controller.arn
}
