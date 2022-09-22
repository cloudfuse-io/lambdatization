# lambdatization (l12n)

<h4 align="center">

[![L12N](https://github.com/cloudfuse-io/lambdatization/actions/workflows/l12n.yaml/badge.svg?branch=main)](https://github.com/cloudfuse-io/lambdatization/actions/workflows/l12n.yaml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

</h4>

The goal of this project is to assess what process can realistically run inside
lambda and have a first feeling about there performances. To do this we will try
to execute a list of engines as Lambdas.

## Lambdatizating yourself

To get started:

- you must have docker installed, it is the only dependency
- clone this repository
- `cd` into it
- run `L12N_BUILD=. ./l12n-shell`
  - the `L12N_BUILD` environment variable indicates to the `l12n-shell` script that it
    needs to build the image
  - the `./l12n-shell` without any argument runs an interactive bash terminal in the
    CLI container
  - the same arguments can be provided to `./l12n-shell` as to `bash`, e.g
    `./l12n-shell -c "cmd"` and `echo "cmd" | ./l12n-shell` both run `cmd` in
    the CLI container

## Contribute

We try to follow the [conventional commits standard](https://www.conventionalcommits.org/en/v1.0.0/).
