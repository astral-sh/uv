FROM gentoo/python
RUN wget --progress=dot:mega -O - https://github.com/gentoo-mirror/gentoo/archive/master.tar.gz | tar -xz \
 && mv gentoo-master /var/db/repos/gentoo
RUN getuto
RUN printf "[binhost]\nsync-uri = https://distfiles.gentoo.org/releases/amd64/binpackages/23.0/x86-64/\n" > /etc/portage/binrepos.conf/gentoobinhost.conf
RUN emerge -vng dev-vcs/git dev-lang/rust-bin dev-build/cmake dev-build/ninja --jobs
COPY . uv/
RUN cd uv && cargo test --no-default-features --features=python,pypi -p uv --test it run_repeated || exit 1
