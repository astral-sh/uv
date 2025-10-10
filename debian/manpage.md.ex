% uv(SECTION) | User Commands
%
% "October 10 2025"

[comment]: # The lines above form a Pandoc metadata block. They must be
[comment]: # the first ones in the file.
[comment]: # See https://pandoc.org/MANUAL.html#metadata-blocks for details.

[comment]: # pandoc -s -f markdown -t man package.md -o package.1
[comment]: # 
[comment]: # A manual page package.1 will be generated. You may view the
[comment]: # manual page with: nroff -man package.1 | less. A typical entry
[comment]: # in a Makefile or Makefile.am is:
[comment]: # 
[comment]: # package.1: package.md
[comment]: #         pandoc --standalone --from=markdown --to=man $< --output=$@
[comment]: # 
[comment]: # The pandoc binary is found in the pandoc package. Please remember
[comment]: # that if you create the nroff version in one of the debian/rules
[comment]: # file targets, such as build, you will need to include pandoc in
[comment]: # your Build-Depends control field.

[comment]: # lowdown is a low dependency, lightweight alternative to
[comment]: # pandoc as a markdown to manpage translator. Use with:
[comment]: # 
[comment]: # package.1: package.md
[comment]: #         lowdown -s -Tman -o $@ $<
[comment]: # 
[comment]: # And add lowdown to the Build-Depends control field.

[comment]: # Remove the lines starting with '[comment]:' in this file in order
[comment]: # to avoid warning messages.

# NAME

uv - program to do something

# SYNOPSIS

**uv** **-e** _this_ [**\-\-example=that**] [{**-e** | **\-\-example**} _this_]
                 [{**-e** | **\-\-example**} {_this_ | _that_}]

**uv** [{**-h** | *\-\-help**} | {**-v** | **\-\-version**}]

# DESCRIPTION

This manual page documents briefly the **uv** and **bar** commands.

This manual page was written for the Debian distribution because the
original program does not have a manual page. Instead, it has documentation
in the GNU info(1) format; see below.

**uv** is a program that...

# OPTIONS

The program follows the usual GNU command line syntax, with long options
starting with two dashes ('-'). A summary of options is included below. For
a complete description, see the **info**(1) files.

**-e** _this_, **\-\-example=**_that_
:   Does this and that.

**-h**, **\-\-help**
:   Show summary of options.

**-v**, **\-\-version**
:   Show version of program.

# FILES

/etc/foo.conf
:   The system-wide configuration file to control the behaviour of
    uv. See **foo.conf**(5) for further details.

${HOME}/.foo.conf
:   The per-user configuration file to control the behaviour of
    uv. See **foo.conf**(5) for further details.

# ENVIRONMENT

**FOO_CONF**
:   If used, the defined file is used as configuration file (see also
    the section called “FILES”).

# DIAGNOSTICS

The following diagnostics may be issued on stderr:

Bad configuration file. Exiting.
:   The configuration file seems to contain a broken configuration
    line. Use the **\-\-verbose** option, to get more info.

**uv** provides some return codes, that can be used in scripts:

    Code Diagnostic
    0 Program exited successfully.
    1 The configuration file seems to be broken.

# BUGS

The program is currently limited to only work with the foobar library.

The upstream BTS can be found at http://bugzilla.foo.tld.

# SEE ALSO

**bar**(1), **baz**(1), **foo.conf**(5)

The programs are documented fully by The Rise and Fall of a Fooish Bar
available via the **info**(1) system.

# AUTHOR

drozdov.m <openfix@example.com>
:   Wrote this manpage for the Debian system.

# COPYRIGHT

Copyright © 2007 drozdov.m

This manual page was written for the Debian system (and may be used by
others).

Permission is granted to copy, distribute and/or modify this document under
the terms of the GNU General Public License, Version 2 or (at your option)
any later version published by the Free Software Foundation.

On Debian systems, the complete text of the GNU General Public License
can be found in /usr/share/common-licenses/GPL.

[comment]: #  Local Variables:
[comment]: #  mode: markdown
[comment]: #  End:
