<?xml version='1.0' encoding='UTF-8'?>
<!DOCTYPE refentry PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN"
"http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd" [

<!--

`xsltproc -''-nonet \
          -''-param man.charmap.use.subset "0" \
          -''-param make.year.ranges "1" \
          -''-param make.single.year.ranges "1" \
          /usr/share/xml/docbook/stylesheet/docbook-xsl/manpages/docbook.xsl \
          manpage.xml`

A manual page <package>.<section> will be generated. You may view the
manual page with: nroff -man <package>.<section> | less'. A typical entry
in a Makefile or Makefile.am is:

DB2MAN = /usr/share/sgml/docbook/stylesheet/xsl/docbook-xsl/manpages/docbook.xsl
XP     = xsltproc -''-nonet -''-param man.charmap.use.subset "0"

manpage.1: manpage.xml
        $(XP) $(DB2MAN) $<

The xsltproc binary is found in the xsltproc package. The XSL files are in
docbook-xsl. A description of the parameters you can use can be found in the
docbook-xsl-doc-* packages. Please remember that if you create the nroff
version in one of the debian/rules file targets (such as build), you will need
to include xsltproc and docbook-xsl in your Build-Depends control field.
Alternatively use the xmlto command/package. That will also automatically
pull in xsltproc and docbook-xsl.

Notes for using docbook2x: docbook2x-man does not automatically create the
AUTHOR(S) and COPYRIGHT sections. In this case, please add them manually as
<refsect1> ... </refsect1>.

To disable the automatic creation of the AUTHOR(S) and COPYRIGHT sections
read /usr/share/doc/docbook-xsl/doc/manpages/authors.html. This file can be
found in the docbook-xsl-doc-html package.

Validation can be done using: `xmllint -''-noout -''-valid manpage.xml`

General documentation about man-pages and man-page-formatting:
man(1), man(7), http://www.tldp.org/HOWTO/Man-Page/

-->

  <!-- Fill in your name for FIRSTNAME and SURNAME. -->
  <!ENTITY dhfirstname "FIRSTNAME">
  <!ENTITY dhsurname   "SURNAME">
  <!-- dhusername could also be set to "&dhfirstname; &dhsurname;". -->
  <!ENTITY dhusername  "drozdov.m">
  <!ENTITY dhemail     "openfix@example.com">
  <!-- SECTION should be 1-8, maybe w/ subsection other parameters are
       allowed: see man(7), man(1) and
       http://www.tldp.org/HOWTO/Man-Page/q2.html. -->
  <!ENTITY dhsection   "SECTION">
  <!-- TITLE should be something like "User commands" or similar (see
       http://www.tldp.org/HOWTO/Man-Page/q2.html). -->
  <!ENTITY dhtitle     "uv User Manual">
  <!ENTITY dhucpackage "Uv">
  <!ENTITY dhpackage   "uv">
]>

<refentry>
  <refentryinfo>
    <title>&dhtitle;</title>
    <productname>&dhpackage;</productname>
    <authorgroup>
      <author>
       <firstname>&dhfirstname;</firstname>
        <surname>&dhsurname;</surname>
        <contrib>Wrote this manpage for the Debian system.</contrib>
        <address>
          <email>&dhemail;</email>
        </address>
      </author>
    </authorgroup>
    <copyright>
      <year>2007</year>
      <holder>&dhusername;</holder>
    </copyright>
    <legalnotice>
      <para>This manual page was written for the Debian system
        (and may be used by others).</para>
      <para>Permission is granted to copy, distribute and/or modify this
        document under the terms of the GNU General Public License,
        Version 2 or (at your option) any later version published by
        the Free Software Foundation.</para>
      <para>On Debian systems, the complete text of the GNU General Public
        License can be found in
        <filename>/usr/share/common-licenses/GPL</filename>.</para>
    </legalnotice>
  </refentryinfo>
  <refmeta>
    <refentrytitle>&dhucpackage;</refentrytitle>
    <manvolnum>&dhsection;</manvolnum>
  </refmeta>
  <refnamediv>
    <refname>&dhpackage;</refname>
    <refpurpose>program to do something</refpurpose>
  </refnamediv>
  <refsynopsisdiv>
    <cmdsynopsis>
      <command>&dhpackage;</command>
      <!-- These are several examples, how syntaxes could look -->
      <arg choice="plain"><option>-e <replaceable>this</replaceable></option></arg>
      <arg choice="opt"><option>--example=<parameter>that</parameter></option></arg>
      <arg choice="opt">
        <group choice="req">
          <arg choice="plain"><option>-e</option></arg>
          <arg choice="plain"><option>--example</option></arg>
        </group>
        <replaceable class="option">this</replaceable>
      </arg>
      <arg choice="opt">
        <group choice="req">
          <arg choice="plain"><option>-e</option></arg>
          <arg choice="plain"><option>--example</option></arg>
        </group>
        <group choice="req">
          <arg choice="plain"><replaceable>this</replaceable></arg>
          <arg choice="plain"><replaceable>that</replaceable></arg>
        </group>
      </arg>
    </cmdsynopsis>
    <cmdsynopsis>
      <command>&dhpackage;</command>
      <!-- Normally the help and version options make the programs stop
           right after outputting the requested information. -->
      <group choice="opt">
        <arg choice="plain">
          <group choice="req">
            <arg choice="plain"><option>-h</option></arg>
            <arg choice="plain"><option>--help</option></arg>
          </group>
        </arg>
        <arg choice="plain">
          <group choice="req">
            <arg choice="plain"><option>-v</option></arg>
            <arg choice="plain"><option>--version</option></arg>
          </group>
        </arg>
      </group>
    </cmdsynopsis>
  </refsynopsisdiv>
  <refsect1 id="description">
    <title>DESCRIPTION</title>
    <para>This manual page documents briefly the
      <command>&dhpackage;</command> and <command>bar</command>
      commands.</para>
    <para>This manual page was written for the Debian distribution
      because the original program does not have a manual page.
      Instead, it has documentation in the GNU <citerefentry>
        <refentrytitle>info</refentrytitle>
        <manvolnum>1</manvolnum>
      </citerefentry> format; see below.</para>
    <para><command>&dhpackage;</command> is a program that...</para>
  </refsect1>
  <refsect1 id="options">
    <title>OPTIONS</title>
    <para>The program follows the usual GNU command line syntax,
      with long options starting with two dashes ('-').  A summary of
      options is included below.  For a complete description, see the
      <citerefentry>
        <refentrytitle>info</refentrytitle>
        <manvolnum>1</manvolnum>
      </citerefentry> files.</para>
    <variablelist>
      <!-- Use the variablelist.term.separator and the
           variablelist.term.break.after parameters to
           control the term elements. -->
      <varlistentry>
        <term><option>-e <replaceable>this</replaceable></option></term>
        <term><option>--example=<replaceable>that</replaceable></option></term>
        <listitem>
          <para>Does this and that.</para>
        </listitem>
      </varlistentry>
      <varlistentry>
        <term><option>-h</option></term>
        <term><option>--help</option></term>
        <listitem>
          <para>Show summary of options.</para>
        </listitem>
      </varlistentry>
      <varlistentry>
        <term><option>-v</option></term>
        <term><option>--version</option></term>
        <listitem>
          <para>Show version of program.</para>
        </listitem>
      </varlistentry>
    </variablelist>
  </refsect1>
  <refsect1 id="files">
    <title>FILES</title>
    <variablelist>
      <varlistentry>
        <term><filename>/etc/foo.conf</filename></term>
        <listitem>
          <para>The system-wide configuration file to control the
            behaviour of <application>&dhpackage;</application>. See
            <citerefentry>
              <refentrytitle>foo.conf</refentrytitle>
              <manvolnum>5</manvolnum>
            </citerefentry> for further details.</para>
        </listitem>
      </varlistentry>
      <varlistentry>
        <term><filename>${HOME}/.foo.conf</filename></term>
        <listitem>
          <para>The per-user configuration file to control the
             behaviour of <application>&dhpackage;</application>. See
             <citerefentry>
               <refentrytitle>foo.conf</refentrytitle>
               <manvolnum>5</manvolnum>
             </citerefentry> for further details.</para>
        </listitem>
      </varlistentry>
    </variablelist>
  </refsect1>
  <refsect1 id="environment">
    <title>ENVIRONMENT</title>
    <variablelist>
      <varlistentry>
        <term><envar>FOO_CONF</envar></term>
        <listitem>
          <para>If used, the defined file is used as configuration
            file (see also <xref linkend="files"/>).</para>
        </listitem>
      </varlistentry>
    </variablelist>
  </refsect1>
  <refsect1 id="diagnostics">
    <title>DIAGNOSTICS</title>
    <para>The following diagnostics may be issued
      on <filename class="devicefile">stderr</filename>:</para>
    <variablelist>
      <varlistentry>
        <term><errortext>Bad configuration file. Exiting.</errortext></term>
        <listitem>
          <para>The configuration file seems to contain a broken configuration
            line. Use the <option>--verbose</option> option, to get more info.
          </para>
        </listitem>
      </varlistentry>
    </variablelist>
    <para><command>&dhpackage;</command> provides some return codes, that can
      be used in scripts:</para>
    <segmentedlist>
      <segtitle>Code</segtitle>
      <segtitle>Diagnostic</segtitle>
      <seglistitem>
        <seg><errorcode>0</errorcode></seg>
        <seg>Program exited successfully.</seg>
      </seglistitem>
      <seglistitem>
        <seg><errorcode>1</errorcode></seg>
        <seg>The configuration file seems to be broken.</seg>
      </seglistitem>
    </segmentedlist>
  </refsect1>
  <refsect1 id="bugs">
    <!-- Or use this section to tell about upstream BTS. -->
    <title>BUGS</title>
    <para>The program is currently limited to only work
      with the <package>foobar</package> library.</para>
    <para>The upstreams <acronym>BTS</acronym> can be found
      at <ulink url="http://bugzilla.foo.tld"/>.</para>
  </refsect1>
  <refsect1 id="see_also">
    <title>SEE ALSO</title>
    <!-- In alpabetical order. -->
    <para><citerefentry>
        <refentrytitle>bar</refentrytitle>
        <manvolnum>1</manvolnum>
      </citerefentry>, <citerefentry>
        <refentrytitle>baz</refentrytitle>
        <manvolnum>1</manvolnum>
      </citerefentry>, <citerefentry>
        <refentrytitle>foo.conf</refentrytitle>
        <manvolnum>5</manvolnum>
      </citerefentry></para>
    <para>The programs are documented fully by <citetitle>The Rise and
      Fall of a Fooish Bar</citetitle> available via the <citerefentry>
        <refentrytitle>info</refentrytitle>
        <manvolnum>1</manvolnum>
      </citerefentry> system.</para>
  </refsect1>
</refentry>

