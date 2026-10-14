#!/usr/bin/perl
# usage: rotate.pl <logfile>
# Copies stdin to <logfile>, rotating at 10 MiB and keeping 5 old files (<logfile>.1 is the newest).
use strict;
use warnings;
use IO::Handle;

my ($log) = @ARGV or die "usage: rotate.pl <logfile>\n";
my $max = 10 * 1024 * 1024;
my $keep = 5;

sub open_log {
    open(my $fh, '>>', $log) or die "open $log: $!\n";
    $fh->autoflush(1);
    return $fh;
}

my $fh = open_log();
while (my $line = <STDIN>) {
    print $fh $line;
    next if tell($fh) < $max;
    close $fh;
    for (my $i = $keep - 1; $i >= 1; $i--) {
        rename("$log.$i", "$log." . ($i + 1)) if -e "$log.$i";
    }
    rename($log, "$log.1");
    $fh = open_log();
}
