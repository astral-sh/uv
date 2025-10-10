#
# Regular cron jobs for the uv package.
#
0 4	* * *	root	[ -x /usr/bin/uv_maintenance ] && /usr/bin/uv_maintenance
