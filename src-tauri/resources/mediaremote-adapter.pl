use strict; use warnings; use DynaLoader;
my ($dylib,$fn)=@ARGV; die "usage: <dylib> <fn>\n" unless $dylib && $fn;
my $h=DynaLoader::dl_load_file($dylib,0) or die "load fail\n";
my $s=DynaLoader::dl_find_symbol($h,"mra_$fn") or die "no sym mra_$fn\n";
DynaLoader::dl_install_xsub("main::$fn",$s);
no strict 'refs'; &{"main::$fn"}();
