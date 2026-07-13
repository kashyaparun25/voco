#import <Foundation/Foundation.h>
#import <dlfcn.h>
typedef void (*GetInfoFn)(dispatch_queue_t, void(^)(CFDictionaryRef));
typedef int  (*SendCmdFn)(int, CFDictionaryRef);
static void* mrh(void){ static void* h=NULL; if(!h) h=dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_NOW); return h; }
static double np_rate(void){
    void* h=mrh(); if(!h) return -1;
    GetInfoFn g=(GetInfoFn)dlsym(h,"MRMediaRemoteGetNowPlayingInfo"); if(!g) return -1;
    dispatch_semaphore_t s=dispatch_semaphore_create(0); __block double rate=-1;
    g(dispatch_get_global_queue(0,0), ^(CFDictionaryRef info){
        if(info){ NSDictionary* d=(__bridge NSDictionary*)info; id r=d[@"kMRMediaRemoteNowPlayingInfoPlaybackRate"]; if(r) rate=[r doubleValue]; }
        dispatch_semaphore_signal(s);
    });
    dispatch_semaphore_wait(s, dispatch_time(DISPATCH_TIME_NOW,(int64_t)(2*NSEC_PER_SEC)));
    return rate;
}
static void send_cmd(int c){
    void* h=mrh(); if(!h) return;
    SendCmdFn f=(SendCmdFn)dlsym(h,"MRMediaRemoteSendCommand"); if(!f) return;
    f(c,NULL);
    // MRMediaRemoteSendCommand delivers asynchronously (XPC to mediaremoted).
    // The perl process exits as soon as we return, which drops the command
    // before it's delivered — so spin the runloop briefly to let it flush.
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.4, false);
}
void mra_pause_if_playing(void* cv){ (void)cv; @autoreleasepool{ double r=np_rate(); if(r>0.0){ send_cmd(1); printf("{\"paused\":true}\n"); } else { printf("{\"paused\":false}\n"); } fflush(stdout);} }
void mra_play(void* cv){ (void)cv; @autoreleasepool{ send_cmd(0); printf("{\"ok\":true}\n"); fflush(stdout);} }
void mra_get(void* cv){ (void)cv; @autoreleasepool{ double r=np_rate(); printf("{\"playing\":%s,\"rate\":%f}\n", r>0.0?"true":"false", r); fflush(stdout);} }
