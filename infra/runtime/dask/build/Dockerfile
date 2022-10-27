
ARG DASK_VERSION=2022.10.0-py3.9
ARG FUNCTION_DIR="/function"


FROM daskdev/dask:$DASK_VERSION
ARG FUNCTION_DIR

RUN mamba install -y \
        dask-sql \
        pyarrow \
        s3fs \
        awslambdaric \
    && mamba clean -tipy \
    && find /opt/conda/ -type f,l -name '*.a' -delete \
    && find /opt/conda/ -type f,l -name '*.pyc' -delete \
    && find /opt/conda/ -type f,l -name '*.js.map' -delete \
    && find /opt/conda/lib/python*/site-packages/bokeh/server/static -type f,l -name '*.js' -not -name '*.min.js' -delete \
    && rm -rf /opt/conda/pkgs

RUN mkdir -p ${FUNCTION_DIR}
COPY lambda-handler.py ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
