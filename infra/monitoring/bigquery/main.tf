variable "region_name" {}
variable "project_id" {}

module "env" {
  source = "../../common/env"
}

locals {
  account_id_raw                = "${module.env.module_name}${module.env.stage}"
  stage_underscore              = replace(module.env.stage, "-", "_")
  dataset_id                    = "${module.env.module_name}_metrics_${local.stage_underscore}"
  standalone_durations_table_id = "${module.env.module_name}-standalone-engine-durations-${module.env.stage}"
  scaling_durations_table_id    = "${module.env.module_name}-scaling-durations-${module.env.stage}"
}

provider "google" {
  project = var.project_id
  region  = var.region_name
}

resource "google_service_account" "bigquery_write" {
  account_id   = "bigquery-write-${substr(md5(local.account_id_raw), 0, 6)}"
  display_name = "BigQuery Write"
}

resource "google_project_iam_member" "firestore_owner_binding" {
  role   = "roles/bigquery.dataEditor"
  member = "serviceAccount:${google_service_account.bigquery_write.email}"
}

resource "google_service_account_key" "bigquery_write_key" {
  service_account_id = google_service_account.bigquery_write.name
}

resource "google_bigquery_dataset" "dataset" {
  dataset_id                  = local.dataset_id
  friendly_name               = "Lambdatization Metrics"
  default_table_expiration_ms = 1576800000000 # 50 years ;-)
  labels                      = module.env.default_tags
}

resource "google_bigquery_table" "standalone_engine_durations" {
  dataset_id          = google_bigquery_dataset.dataset.dataset_id
  table_id            = local.standalone_durations_table_id
  labels              = module.env.default_tags
  deletion_protection = false

  time_partitioning {
    type  = "DAY"
    field = "timestamp"
  }

  schema = <<EOF
[
  {
    "name": "engine",
    "mode": "NULLABLE",
    "type": "STRING"
  },
  {
    "name": "external_duration_ms",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "cold_start",
    "mode": "NULLABLE",
    "type": "BOOLEAN"
  },
  {
    "name": "timestamp",
    "mode": "NULLABLE",
    "type": "TIMESTAMP"
  }
]
EOF
}

resource "google_bigquery_table" "scaling_durations" {
  dataset_id          = google_bigquery_dataset.dataset.dataset_id
  table_id            = local.scaling_durations_table_id
  labels              = module.env.default_tags
  deletion_protection = false

  time_partitioning {
    type  = "DAY"
    field = "timestamp"
  }

  schema = <<EOF
[
  {
    "name": "placeholder_size_mb",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "corrected_duration_ms",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "nb_run",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "nb_cold_start",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "memory_size_mb",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "timestamp",
    "mode": "NULLABLE",
    "type": "TIMESTAMP"
  }
]
EOF
}

output "service_account_key" {
  value     = google_service_account_key.bigquery_write_key.private_key
  sensitive = true
}

output "standalone_durations_table_id" {
  value = "${var.project_id}.${local.dataset_id}.${local.standalone_durations_table_id}"
}

output "scaling_durations_table_id" {
  value = "${var.project_id}.${local.dataset_id}.${local.scaling_durations_table_id}"
}
