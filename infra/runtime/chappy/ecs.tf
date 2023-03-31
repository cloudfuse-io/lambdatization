locals {
  seed_port = 8000
}

module "vpc" {
  source = "terraform-aws-modules/vpc/aws"

  name = "${module.env.module_name}-chappydev-${var.region_name}-${module.env.stage}"
  cidr = "10.0.0.0/28"

  azs            = ["${var.region_name}a"]
  public_subnets = ["10.0.0.0/28"]
}

resource "aws_ecs_cluster" "cluster" {
  name = "${module.env.module_name}-chappydev-${var.region_name}-${module.env.stage}"
}


resource "aws_iam_role" "ecs_task_execution_role" {
  name = "${module.env.module_name}-chappydev-task-exec-${var.region_name}-${module.env.stage}"

  assume_role_policy = <<EOF
{
 "Version": "2012-10-17",
 "Statement": [
   {
     "Action": "sts:AssumeRole",
     "Principal": {
       "Service": "ecs-tasks.amazonaws.com"
     },
     "Effect": "Allow",
     "Sid": ""
   }
 ]
}
EOF
}

resource "aws_iam_role_policy_attachment" "ecs-task-execution-role-policy-attachment" {
  role       = aws_iam_role.ecs_task_execution_role.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}


## seed ##

resource "aws_cloudwatch_log_group" "seed" {
  name = "/ecs/gateway/${module.env.module_name}-chappydev-seed-${module.env.stage}"
}


resource "aws_iam_role" "ecs_task_role" {
  name = "${module.env.module_name}-chappydev-seed-task-${var.region_name}-${module.env.stage}"

  assume_role_policy = <<EOF
{
 "Version": "2012-10-17",
 "Statement": [
   {
     "Action": "sts:AssumeRole",
     "Principal": {
       "Service": "ecs-tasks.amazonaws.com"
     },
     "Effect": "Allow",
     "Sid": ""
   }
 ]
}
EOF
}

resource "aws_iam_role_policy" "fargate_task_policy" {
  name = "${module.env.module_name}-chappydev-seed-task-${var.region_name}-${module.env.stage}"
  role = aws_iam_role.ecs_task_role.id

  policy = <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "ssmmessages:CreateControlChannel",
        "ssmmessages:CreateDataChannel",
        "ssmmessages:OpenControlChannel",
        "ssmmessages:OpenDataChannel"
      ],
      "Resource": "*"
    },
    {
      "Sid": "objectlevel",
      "Effect": "Allow",
      "Action": "s3:*",
      "Resource": "${var.bucket_arn}/*"
    },
    {
      "Sid": "bucketlevel",
      "Effect": "Allow",
      "Action": "s3:*",
      "Resource": "${var.bucket_arn}"
    }
  ]
}
EOF
}

resource "aws_ecs_task_definition" "chappydev_seed" {
  family                   = "${module.env.module_name}-chappydev-seed-task-${module.env.stage}"
  task_role_arn            = aws_iam_role.ecs_task_role.arn
  execution_role_arn       = aws_iam_role.ecs_task_execution_role.arn
  network_mode             = "awsvpc"
  cpu                      = "512"
  memory                   = "1024"
  requires_compatibilities = ["FARGATE"]
  container_definitions    = <<DEFINITION
[
  {
    "image": "${var.chappydev_image}",
    "name": "server",
    "logConfiguration": {
      "logDriver": "awslogs",
      "options": {
        "awslogs-region" : "${var.region_name}",
        "awslogs-group" : "${aws_cloudwatch_log_group.seed.name}",
        "awslogs-stream-prefix" : "ecs"
      }
    },          
    "environment": [{
        "name": "PORT",
        "value": "${local.seed_port}"
      },{
        "name": "RUST_LOG",
        "value": "debug,h2=error"
      },{
        "name": "RUST_BACKTRACE",
        "vallue": "1"
    }],
    "entrypoint": ["sleep", "infinity"]
  },
  {
    "image": "openzipkin/zipkin:2.24",
    "name": "opentelemetry",
    "logConfiguration": {
      "logDriver": "awslogs",
      "options": {
        "awslogs-region" : "${var.region_name}",
        "awslogs-group" : "${aws_cloudwatch_log_group.seed.name}",
        "awslogs-stream-prefix" : "ecs"
      }
    },          
    "environment": []
  }
]
DEFINITION
}

resource "aws_security_group" "seed_all" {
  name        = "${module.env.module_name}-chappydev-seed-${module.env.stage}"
  description = "Allow inbound port for GRPC and all outbound"
  vpc_id      = module.vpc.vpc_id

  ingress {
    protocol    = "tcp"
    from_port   = local.seed_port
    to_port     = local.seed_port
    cidr_blocks = ["0.0.0.0/0"]
  }

  # ZIPKIN
  ingress {
    protocol    = "tcp"
    from_port   = 9411
    to_port     = 9411
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_ecs_service" "chappydev_seed" {
  name                               = "${module.env.module_name}-chappydev-seed-${module.env.stage}"
  cluster                            = aws_ecs_cluster.cluster.name
  task_definition                    = aws_ecs_task_definition.chappydev_seed.arn
  desired_count                      = 0
  deployment_maximum_percent         = 100
  deployment_minimum_healthy_percent = 0
  propagate_tags                     = "SERVICE"
  enable_execute_command             = true
  enable_ecs_managed_tags            = true
  launch_type                        = "FARGATE"

  network_configuration {
    subnets          = module.vpc.public_subnets
    security_groups  = [aws_security_group.seed_all.id]
    assign_public_ip = true
  }

  lifecycle {
    ignore_changes = [desired_count]
  }
}

## outputs ##

output "fargate_cluster_name" {
  value = aws_ecs_cluster.cluster.name
}

output "seed_task_family" {
  value = aws_ecs_task_definition.chappydev_seed.family
}

output "seed_service_name" {
  value = aws_ecs_service.chappydev_seed.name
}
