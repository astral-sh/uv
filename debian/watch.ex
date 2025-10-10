# Example watch control file for uscan.
# Rename this file to "watch" and then you can run the "uscan" command
# to check for upstream updates and more.
# See uscan(1) for format.

# Compulsory line, this is a version 4 file.
version=4

# PGP signature mangle, so foo.tar.gz has foo.tar.gz.sig.
#opts="pgpsigurlmangle=s%$%.sig%"

# HTTP site (basic).
#http://example.com/downloads.html \
#    files/uv-([\d\.]+)\.tar\.gz

# Uncomment to examine an FTP server.
#ftp://ftp.example.com/pub/uv-(.*)\.tar\.gz

# SourceForge hosted projects.
#http://sf.net/uv/ uv-(.*)\.tar\.gz

# GitHub hosted projects.
#opts="filenamemangle=s%(?:.*?)?v?(@ANY_VERSION@@ARCHIVE_EXT@)%@PACKAGE@-$1%" \
#    https://github.com/<user>/<project>/tags \
#    (?:.*?/)v?@ANY_VERSION@@ARCHIVE_EXT@

# GitLab hosted projects.
#opts="filenamemangle=s%(?:.*?)?v?(@ANY_VERSION@@ARCHIVE_EXT@)%@PACKAGE@-$1%" \
#    https://gitlab.com/<user>/<project>/-/tags \
#    archive/v?@ANY_VERSION@/<project>-v?\d\S*@ARCHIVE_EXT@

# PyPI.
#https://pypi.debian.net/uv/uv-(.+)\.(?:zip|tgz|tbz|txz|(?:tar\.(?:gz|bz2|xz)))

# Direct Git.
#opts="mode=git" http://git.example.com/uv.git \
#    refs/tags/v([\d\.]+)
