#!/usr/bin/perl
# usage: rotate.pl <logfile>
# Copies stdin to <logfile>, rotating at 10 MiB and keeping 5 old files (<logfile>.1 is the newest).
# It must never stop reading: the node writes its log into this pipe and would block if we did.
# So write or rotation failures (e.g. a full disk) drop lines instead of stalling, and one record is
# capped at 64 KiB.
use strict;
use warnings;
use IO::Handle;

my ($log) = @ARGV or die "usage: rotate.pl <logfile>\n";
my $max = 10 * 1024 * 1024;
my $keep = 5;
my $max_line = 64 * 1024;

sub open_log {
    open(my $fh, '>>', $log) or return undef;
    $fh->autoflush(1);
    return $fh;
}

sub rotate {
    for (my $i = $keep - 1; $i >= 1; $i--) {
        rename("$log.$i", "$log." . ($i + 1)) if -e "$log.$i";
    }
    rename($log, "$log.1") or print STDERR "rotate.pl: rename $log: $!\n";
}

my $fh = open_log();
my $dropped = 0;
while (my $line = <STDIN>) {
    $line = substr($line, 0, $max_line) . "...[truncated]\n" if length($line) > $max_line;
    $fh //= open_log();
    if (!$fh || !print $fh $line) {
        $dropped++;
        $fh = undef;    # reopen on the next line
        next;
    }
    if ($dropped) {
        print $fh "rotate.pl: dropped $dropped log lines after write failures\n";
        $dropped = 0;
    }
    next if tell($fh) < $max;
    close $fh;
    rotate();
    $fh = open_log();
}
