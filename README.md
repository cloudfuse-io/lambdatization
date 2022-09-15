# lambdatization

The goal of this project is to assess what process can realistically run inside
lambda and have a first feeling about there performances. To do this we will try
to execute a list of engines as Lambdas.

## Lambdatizating, yourself

To get started:

- you must have docker installed, it is the only dependency
- clone this repository
- `cd` into it
- run `L12N_BUILD=. ./l12n`
  - the `L12N_BUILD` environment variable indicates to the `l12n` script that it
    needs to build the image
  - the `./l12n` without any argument runs an interactive bash terminal in the
    CLI container
