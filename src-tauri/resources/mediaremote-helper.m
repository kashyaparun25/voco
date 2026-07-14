#import <Foundation/Foundation.h>
#import <dlfcn.h>
typedef void (*GetInfoFn)(dispatch_queue_t, void(^)(CFDictionaryRef));
typedef void (*IsPlayingFn)(dispatch_queue_t, void(^)(Boolean));
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
// Authoritative transport state. Unlike the now-playing *playback rate* (which
// can read stale-high for a window after the user manually pauses — causing us
// to "pause" already-paused media and then wrongly resume it), IsPlaying tracks
// the live play/pause transport and flips immediately on a manual pause.
// Returns 1 (playing), 0 (paused/stopped), or -1 (symbol unavailable / no reply).
static int np_is_playing(void){
    void* h=mrh(); if(!h) return -1;
    IsPlayingFn f=(IsPlayingFn)dlsym(h,"MRMediaRemoteGetNowPlayingApplicationIsPlaying"); if(!f) return -1;
    dispatch_semaphore_t s=dispatch_semaphore_create(0); __block int playing=-1;
    f(dispatch_get_global_queue(0,0), ^(Boolean b){ playing = b ? 1 : 0; dispatch_semaphore_signal(s); });
    dispatch_semaphore_wait(s, dispatch_time(DISPATCH_TIME_NOW,(int64_t)(2*NSEC_PER_SEC)));
    return playing;
}
// True iff media is actually playing right now. Prefer IsPlaying; fall back to
// the playback rate only when IsPlaying is genuinely unavailable (symbol missing
// or no reply), never merely because it reports "not playing".
static int playing_now(void){
    int p = np_is_playing();
    if (p >= 0) return p;
    return np_rate() > 0.0 ? 1 : 0;
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
void mra_pause_if_playing(void* cv){ (void)cv; @autoreleasepool{
    // 1) Only consider pausing if something is actually playing right now.
    if (playing_now() != 1) { printf("{\"paused\":false}\n"); fflush(stdout); return; }
    // 2) Pause, then CONFIRM playback actually stopped as a result. If it still
    //    reads "playing" afterwards, the first reading was stale (the user had
    //    already paused it) or the command didn't land — either way we must NOT
    //    take ownership, so we never resume media we didn't truly pause.
    send_cmd(1);            // spins the runloop ~0.4s so the command is delivered
    usleep(150000);         // small extra settle for the state to reflect
    int after = playing_now();
    printf("{\"paused\":%s}\n", after == 0 ? "true" : "false");
    fflush(stdout);
} }
void mra_play(void* cv){ (void)cv; @autoreleasepool{ send_cmd(0); printf("{\"ok\":true}\n"); fflush(stdout);} }
void mra_get(void* cv){ (void)cv; @autoreleasepool{ int p=np_is_playing(); double r=np_rate(); printf("{\"playing\":%s,\"isPlaying\":%d,\"rate\":%f}\n", playing_now()==1?"true":"false", p, r); fflush(stdout);} }
