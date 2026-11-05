#!/usr/bin/env python3
"""Validate compact dual-screen browsing, explicit upcoming queue and audio in DSP LLE.

Uses generated media, an isolated SD image and a copy of existing local NAND.
"""
from pathlib import Path
import argparse
import csv
import hashlib
import json
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
NAMES = ['01 - Beyoncé - Déjà vu.m4a','02 - Zażółć gęślą jaźń.m4a',
         '03 - Привет мир.m4a','04 - 日本語の歌.m4a','05 - 中国音乐.m4a']
NAMES += [f'Track {i:03}.m4a' for i in range(5,200)]

def path_hash(text):
    value=2166136261
    for byte in text.encode('utf-8'): value=((value^byte)*16777619)&0xffffffff
    return value

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'output/emulator/compact-release-lle')
    parser.add_argument('--bios-dir',type=Path,default=Path.home()/'Downloads/DS firmware + DSi NAND and firmware')
    parser.add_argument('--analyze-only',action='store_true')
    args=parser.parse_args(); out=args.output.resolve(); out.mkdir(parents=True,exist_ok=True)
    if not args.analyze_only:
        fixture=ROOT/'output/emulator-library'; sd=fixture/'sd'; music=sd/'音楽 Music'; music.mkdir(parents=True,exist_ok=True)
        tone=fixture/'chirp.m4a'
        if not tone.exists():
            subprocess.run(['ffmpeg','-nostdin','-hide_banner','-loglevel','error','-y','-f','lavfi',
                '-i','aevalsrc=0.12*sin(2*PI*(220*t+10*t*t)):s=44100:d=40','-ac','2','-c:a','aac','-profile:a','aac_low','-b:a','128k',str(tone)],check=True)
        for name in NAMES:
            dest=music/name
            if not dest.exists(): shutil.copyfile(tone,dest)
        events=[]
        def event(frame,kind,*values): events.append((frame,kind,*values))
        def key(frame,mask): event(frame,'keys',mask);event(frame+2,'keys',0)
        def touch(frame,x,y):event(frame,'touch',x,y);event(frame+3,'release')
        key(120,1);key(160,1) # enter folder, play first song: queue remains empty
        key(190,4);event(193,'shot');key(194,2) # view the genuinely empty upcoming queue
        for frame in [200,220,240,260]:key(frame,128);key(frame+5,2048)
        key(280,4);event(290,'shot') # queue selected last item, current audio unchanged
        touch(300,30,183);event(310,'shot') # E -> top: E B C D
        touch(320,70,183) # E -> second: B E C D
        touch(340,130,183) # E -> bottom: B C D E
        key(360,64);key(370,2048) # remove D: B C E
        event(380,'shot');touch(390,178,7) # queue header ADD returns to library
        # Thumb drag to bottom, then top. Neither gesture opens a media file.
        event(410,'touch',248,37);event(416,'touch',248,135);event(422,'release');event(430,'shot')
        event(440,'touch',248,135);event(446,'touch',248,32);event(452,'release');event(460,'shot')
        touch(480,135,7) # FIND
        for frame,(x,y) in zip(range(490,539,8),[(44,132),(116,132),(116,166),(116,149),(92,149),(68,132),(116,132)]):touch(frame,x,y)
        event(550,'shot');key(560,2)
        # Seek right to the end, allowing natural completion to consume B next.
        event(580,'touch',238,152);event(586,'release');event(635,'shot')
        key(650,256);key(700,256);key(750,256) # C, E, then automatic folder track 1
        # No further input for over ten seconds; top returns to now-playing.
        event(1320,'shot');event(1370,'shot')
        key(1400,128);event(1410,'shot') # browsing expands again, even with filter
        touch(1420,135,7);touch(1430,68,132);event(1440,'shot') # edit query restarts timer
        key(1450,2)
        # Headphone/lid behavior must survive the renderer/controller changes.
        event(1470,'headphones',1);event(1480,'lid',1)
        event(1540,'headphones',0);event(1600,'headphones',1)
        event(1660,'lid',0);key(1680,8);event(1690,'lid',1)
        event(1730,'lid',0);event(1740,'shot');key(1750,8);event(1790,'shot')
        ef=out/'events.tsv';ef.write_text('\n'.join(' '.join(map(str,e)) for e in sorted(events))+'\n')
        nm='/opt/wonderful/toolchain/gcc-arm-none-eabi/bin/arm-none-eabi-nm';elf=ROOT/'target/armv5te-nintendo-ds/release/sounda.elf'
        symbols=subprocess.check_output([nm,str(elf)],text=True)
        address=next('0x'+line.split()[0] for line in symbols.splitlines() if line.endswith(' sounda_diagnostics'))
        command=[str(ROOT/'output/emulator/build/sounda-emulator'),str(ROOT/'output/sounda.nds'),str(args.bios_dir),str(sd),str(out),'1800',str(ef),'0',address,'lle']
        (out/'command.json').write_text(json.dumps(command,indent=2)+'\n')
        (out/'rom.sha256').write_text(hashlib.sha256((ROOT/'output/sounda.nds').read_bytes()).hexdigest()+'\n')
        with (out/'emulator.log').open('w') as log:subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
    analyze(out)

def analyze(out):
    import numpy as np
    from PIL import Image
    rows=list(csv.DictReader((out/'diagnostics.csv').open()));trace=list(csv.DictReader((out/'trace.csv').open()))
    runner=json.loads((out/'runner.json').read_text())
    def w(f,i):return int(rows[f][f'word{i}'])
    def song(i):return path_hash('sd:/音楽 Music/'+NAMES[i])
    checks={
        'completed_frames':len(rows)==1800,
        'original_firmware_lle':not runner['hle_aac_decoder'] and runner['host_aac_decode_calls']==0,
        'no_core_exception':not runner['core_exception'],
        'directory_unicode_200':w(155,12)==200,
        'automatic_folder_does_not_fill_queue':w(190,21)==0 and w(190,22)==0,
        'initial_empty_queue_view':w(193,20)==1 and w(193,21)==0 and w(193,38)==1,
        'explicit_four_added':w(278,21)==4 and w(278,37)==song(0),
        'queue_edits_do_not_restart_audio':w(190,2)==w(380,2),
        'queue_top':w(310,39)==song(4) and w(310,36)==0,
        'queue_second':w(332,39)==song(1) and w(332,36)==1,
        'queue_bottom':w(355,39)==song(1) and w(355,36)==3,
        'queue_remove':w(380,21)==3,
        'queue_header_add_returns_to_library':w(400,20)==0 and w(400,21)==3,
        'scrollbar_bottom':w(430,34)>=184 and w(430,33)>=184,
        'scrollbar_top':w(460,34)==0,
        'scrolling_does_not_play_or_seek':w(460,2)==w(380,2) and w(460,30)==0,
        'normal_expanded_capacity_14':w(460,35)==14 and w(460,32)==1,
        'search_expanded_capacity_13':w(550,35)==13 and w(550,32)==1,
        'accent_search_one_result':w(550,29)==1 and w(550,28)==0,
        'seek_commits_once':w(635,30)==1,
        'natural_eof_uses_first_explicit_song':w(635,37)==song(1) and w(635,21)==2,
        'next_uses_second_explicit_song':w(680,37)==song(2) and w(680,21)==1,
        'next_uses_last_explicit_song':w(730,37)==song(4) and w(730,21)==0,
        'empty_queue_resumes_folder_cursor':w(780,37)==song(1) and w(780,21)==0,
        'before_ten_second_timeout':w(1320,32)==1,
        'after_ten_second_timeout':w(1370,32)==0 and w(1370,35)==6,
        'moving_list_reexpands_top':w(1410,32)==1,
        'search_input_reexpands_top':w(1440,32)==1 and w(1440,20)==2,
        'manual_pause_survives_lid':w(1740,31)==1,
        'no_pcm_underruns':max(int(r['word5']) for r in rows)==0,
    }
    for label,a,b,state,decoding in [('closed_headphones',1490,1530,1,True),('closed_unplugged',1550,1590,2,False),('closed_replugged',1610,1650,1,True)]:
        checks[label+'_state']=all(w(f,26)==state for f in range(a,b))
        checks[label+'_decoder']=(w(b,4)>w(a,4)) if decoding else (w(b,4)==w(a,4))
        checks[label+'_ui_suspended']=w(b,10)==w(a,10)
    pcm=np.fromfile(out/'audio.s16le',dtype='<i2').reshape(-1,2);offsets=np.cumsum([0]+[int(r['audio_frames']) for r in trace]);audio={}
    for label,a,b,silent in [('idle_playback',900,1250,False),('closed_headphones',1490,1530,False),('closed_unplugged',1550,1590,True),('closed_replugged',1610,1650,False),('manual_pause',1735,1745,True),('resumed',1770,1790,False)]:
        part=pcm[offsets[a]:offsets[b]].astype(float);rms=float(np.sqrt(np.mean(part*part)))
        audio[label]={'rms':round(rms,3),'samples':len(part)}
        checks['audio_'+label]=(rms<1) if silent else (rms>100)
    for label,a,b in [('while_dragging_list',410,460),('across_idle_restore',1330,1370)]:
        part=pcm[offsets[a]:offsets[b]]
        count=len(part)//48
        silent=np.max(np.abs(part[:count*48].astype(np.int32).reshape(count,48,2)),axis=(1,2))<=1
        audio[label]={'digital_silence_ms':int(silent.sum()),'complete_ms':count}
        checks[label+'_continuous_pcm']=not silent.any()
    for path in out.glob('frame-*.ppm'):Image.open(path).save(path.with_suffix('.png'))
    def crop(f,box):return np.array(Image.open(out/f'frame-{f:05}.ppm').crop(box))
    checks['idle_top_pixels_return_to_player']=not np.array_equal(crop(1320,(8,44,239,188)),crop(1370,(8,44,239,188)))
    checks['clock_updates']=not np.array_equal(crop(900,(190,5,252,25)),crop(1320,(190,5,252,25)))
    report={'rom_sha256':(out/'rom.sha256').read_text().strip(),'checks':checks,'passed':all(checks.values()),'audio':audio,
        'decoded_frames':w(len(rows)-1,4),'dsp_boots':w(len(rows)-1,2),'maximum_observed_underruns':max(int(r['word5']) for r in rows),'directory_scan_ms':w(155,13)}
    (out/'analysis.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
    if not report['passed']:raise SystemExit('Compact UI/queue checks failed; inspect analysis.json and screenshots')

if __name__=='__main__':main()
