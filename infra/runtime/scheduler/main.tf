variable "region_name" {}

variable "lambdacli_lambda_arn" {}

variable "lambdacli_lambda_name" {}

module "env" {
  source = "../../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

locals {
  # avoid undesired updates of .terraform.lock.hcl that dirty the git status
  init_flags = "--flags='-lockfile=readonly'"
}

## STANDALONE ENGINES

locals {
  standalone_engine_cmd   = "l12n init ${local.init_flags} monitoring.bench-cold-warm"
  standalone_engine_input = jsonencode({ "cmd" : base64encode(local.standalone_engine_cmd), "sampling" : 0.5 })
}

resource "aws_cloudwatch_event_rule" "standalone_engine_schedule" {
  name                = "${module.env.module_name}-standalone-engine-sched-${module.env.stage}"
  description         = "Start standalone engine benchmark"
  schedule_expression = "rate(15 minutes)"
}

resource "aws_cloudwatch_event_target" "standalone_engine_schedule" {
  rule  = aws_cloudwatch_event_rule.standalone_engine_schedule.name
  arn   = var.lambdacli_lambda_arn
  input = local.standalone_engine_input
}

resource "aws_lambda_permission" "allow_standalone_engine" {
  action        = "lambda:InvokeFunction"
  function_name = var.lambdacli_lambda_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.standalone_engine_schedule.arn
}

## LAMBDA SCALE UP

locals {
  scales          = [64, 128, 256]
  scaling_cmds    = [for sc in local.scales : "l12n init ${local.init_flags} monitoring.bench-scaling -n ${sc}"]
  scaling_encoded = [for cmd in local.scaling_cmds : base64encode(cmd)]
  samplings       = [for sc in local.scales : 32 / sc]
  scaling_inputs  = [for i in range(length(local.scales)) : jsonencode({ "cmd" : local.scaling_encoded[i], "sampling" : local.samplings[i] })]
}

resource "aws_cloudwatch_event_rule" "scaling_schedule" {
  count               = length(local.scales)
  name                = "${module.env.module_name}-scaling-sched-${local.scales[count.index]}-${module.env.stage}"
  description         = "Start scaling benchmark with ${local.scales[count.index]} functions"
  schedule_expression = "cron(${count.index * 10 + 4} * * * ? *)"
}

resource "aws_cloudwatch_event_target" "scaling_schedule" {
  count = length(local.scales)
  rule  = aws_cloudwatch_event_rule.scaling_schedule[count.index].name
  arn   = var.lambdacli_lambda_arn
  input = local.scaling_inputs[count.index]
}

resource "aws_lambda_permission" "allow_scaling" {
  count         = length(local.scales)
  action        = "lambda:InvokeFunction"
  function_name = var.lambdacli_lambda_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.scaling_schedule[count.index].arn
}
