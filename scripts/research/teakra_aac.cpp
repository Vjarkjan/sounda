// Runs the operator-supplied linked Teak COFF in the real Teakra interpreter.
// No Nintendo code is incorporated into this harness. Input is ADTS AAC-LC;
// its transport headers are stripped before the DSP command is sent.
#include <teakra/teakra.h>
#ifndef SOUNDA_VENDOR_TEAKRA
#include <teakra/impl/register.h>
#endif
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#ifdef SOUNDA_VENDOR_TEAKRA
#include <Platform.h>
#include <cstdarg>
#include <cstdio>
namespace melonDS::Platform {
void Log(LogLevel, const char* format, ...) {
    va_list args;
    va_start(args, format);
    std::vfprintf(stderr, format, args);
    va_end(args);
}
}
#endif

using Bytes = std::vector<std::uint8_t>;
static Bytes read(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) throw std::runtime_error("cannot open " + path);
    return Bytes(std::istreambuf_iterator<char>(f), {});
}
static std::uint16_t read16(const Bytes& b, std::size_t p) { return b.at(p) | (b.at(p+1) << 8); }
static std::uint32_t read32(const Bytes& b, std::size_t p) { return read16(b,p) | (std::uint32_t(read16(b,p+2)) << 16); }

struct Decoder {
#ifdef SOUNDA_VENDOR_TEAKRA
    Teakra::Teakra dsp;
    Bytes dsp_memory = Bytes(0x80000);
#else
    Teakra::Teakra dsp{Teakra::UserConfig{}};
#endif
    Bytes memory = Bytes(0x1000000);
    std::uint64_t cycles = 0;
    std::uint32_t pointer = 0;
    unsigned decoded_count = 0;
    std::uint64_t ahbm_reads=0, ahbm_writes=0;
    void run(unsigned count=128) {
#ifndef SOUNDA_VENDOR_TEAKRA
        auto before=dsp.GetRegisterState().pc;
#endif
        try { dsp.Run(count); } catch(...) {
#ifndef SOUNDA_VENDOR_TEAKRA
            std::cerr<<"DSP interpreter failure beforePC=0x"<<std::hex<<before<<" opcode=0x"<<dsp.ProgramRead(before)<<" afterPC=0x"<<dsp.GetRegisterState().pc<<" SP=0x"<<dsp.GetRegisterState().sp<<std::dec<<"\n";
#endif
            throw;
        }
        cycles += count;
    }
    std::size_t at(std::uint32_t a, std::size_t n) const {
        if (a < 0x02000000 || std::uint64_t(a) + n > 0x03000000)
            throw std::runtime_error("out-of-range external DMA");
        return a - 0x02000000;
    }
    explicit Decoder(const Bytes& firmware) {
        if (read16(firmware,0) != 0xA4) throw std::runtime_error("not Teak COFF");
        dsp.Reset();
#ifdef SOUNDA_VENDOR_TEAKRA
        Teakra::SharedMemoryCallback shared;
        // The vendored SharedMemory adapter passes BYTE addresses here.
        shared.read16=[this](auto address) { return read16(dsp_memory,address); };
        shared.write16=[this](auto address,auto value) { dsp_memory.at(address)=value;dsp_memory.at(address+1)=value>>8; };
        dsp.SetSharedMemoryCallback(shared);
#endif
        for (unsigned i=0; i<read16(firmware,2); ++i) {
            auto p = 20 + read16(firmware,16) + 40*i;
            auto flags=read32(firmware,p+36), size=read32(firmware,p+16), data=read32(firmware,p+20);
            if ((flags & 0x80) || !size) continue;
            for (bool code : {true,false}) {
                if (!(flags & (code ? 8 : 16))) continue;
                // Dual-placement BLK_HDR records are excluded by their name.
                std::string name;
                bool long_name=read32(firmware,p)==0;
                auto np = long_name ? read32(firmware,8)+18*read32(firmware,12)+read32(firmware,p+4) : p;
                auto end = long_name ? firmware.size() : p+8;
                while (np<end && firmware.at(np)) name += char(firmware.at(np++));
                if (name.find(code ? "@c0" : "@d0")==std::string::npos) continue;
                auto address=read32(firmware,p+(code ? 8 : 12));
                if ((size&1) || address*2+size>0x20000) throw std::runtime_error("invalid placement");
                for (unsigned k=0;k<size/2;++k) {
                    auto value=(flags & (code ? 32 : 64)) ? 0 : read16(firmware,data+2*k);
                    if (code) dsp.ProgramWrite(address+k,value); else dsp.DataWriteA32(address+k,value);
                }
            }
        }
        Teakra::AHBMCallback cb;
        cb.read8=[this](auto a){ ++ahbm_reads;return memory.at(at(a,1)); };
        cb.write8=[this](auto a,auto v){ ++ahbm_writes;memory.at(at(a,1))=v; };
        cb.read16=[this](auto a){ ++ahbm_reads;return read16(memory,at(a,2)); };
        cb.write16=[this](auto a,auto v){++ahbm_writes;auto p=at(a,2);memory[p]=v;memory[p+1]=v>>8;};
        cb.read32=[this](auto a){++ahbm_reads;return read32(memory,at(a,4));};
        cb.write32=[this](auto a,auto v){++ahbm_writes;auto p=at(a,4);for(int i=0;i<4;++i)memory[p+i]=v>>(8*i);};
        dsp.SetAHBMCallback(cb);
        dsp.MaskSemaphore(0xffff);
        unsigned ready=0;
        std::array<unsigned,0x6000> hits{};
        while (ready!=7 && cycles<10000000) {
#ifndef SOUNDA_VENDOR_TEAKRA
            auto pc=dsp.GetRegisterState().pc;
            if(pc<hits.size()) ++hits[pc];
#endif
            run(1);
            for(unsigned i=0;i<3;++i) if (!(ready&(1<<i)) && dsp.RecvDataIsReady(i)) {
                auto value=dsp.RecvData(i);
                if(value!=1) throw std::runtime_error("unexpected boot reply");
                ready|=1<<i;
            }
        }
        if(ready!=7) {
#ifndef SOUNDA_VENDOR_TEAKRA
            std::cerr << "startup ready="<<ready<<" PC=0x"<<std::hex<<dsp.GetRegisterState().pc<<" SP="<<dsp.GetRegisterState().sp<<std::dec<<"\n";
            for(unsigned p : {0x20,0x32,0x34,0x38,0x195,0x264,0x31c,0x3a5,0x45e,0x4df,0x4e8,0x4ed,0x4f1,0xfef,0xff3,0x1e01,0x1e55}) std::cerr<<"pc 0x"<<std::hex<<p<<std::dec<<" hits "<<hits[p]<<"\n";
            for(unsigned p : {0x2a2e,0x2a2f,0x2a43,0x903e,0x904d,0x904e,0x904f,0x9050}) std::cerr<<"data 0x"<<std::hex<<p<<" 0x"<<dsp.DataReadA32(p)<<std::dec<<"\n";
#endif
            throw std::runtime_error("DSP startup timeout");
        }
        std::cout << "{\"event\":\"boot\",\"ready_mask\":" << ready << ",\"cycles\":" << cycles << "}\n";
    }
    void housekeeping() {
        if(dsp.RecvDataIsReady(2)) pointer=dsp.RecvData(2);
        dsp.ClearSemaphore(0xffff);
    }
    void send(std::uint16_t word) {
        auto start=cycles;
        while(!dsp.SendDataIsEmpty(1)) {
            housekeeping(); run();
            if(cycles-start>10000000) throw std::runtime_error("CMD1 timeout");
        }
        dsp.SendData(1,word);
    }
    unsigned decode(const Bytes& frame,unsigned rate,unsigned channels) {
        std::fill(memory.begin(),memory.begin()+2048,0);
        std::copy(frame.begin(),frame.end(),memory.begin());
        for (auto w : std::array<std::uint16_t,11>{1,std::uint16_t(frame.size()),std::uint16_t(rate>>16),std::uint16_t(rate),std::uint16_t(channels),0x0200,0,0x0201,0,0x0201,0x0800}) send(w);
        auto start=cycles;
        while(!dsp.RecvDataIsReady(0)) {
#ifndef SOUNDA_VENDOR_TEAKRA
            auto pc=dsp.GetRegisterState().pc;
            if(decoded_count==0 && (pc==0xd3 || pc==0x772 || pc==0x20a || pc==0x218 || pc==0x226 || pc==0x23e || pc==0x245)) {
                auto&r=dsp.GetRegisterState();std::cerr<<"decode pc="<<std::hex<<pc<<" a0="<<r.a[0]<<" a1="<<r.a[1]<<" b0="<<r.b[0]<<" b1="<<r.b[1]<<" data6000="<<dsp.DataReadA32(0x6000)<<","<<dsp.DataReadA32(0x6001)<<" input="<<read16(memory,0)<<","<<read16(memory,2)<<std::dec<<" reads="<<ahbm_reads<<" writes="<<ahbm_writes<<"\n";
            }
#endif
            housekeeping(); run(1);
            if(cycles-start>10000000) throw std::runtime_error("decode timeout");
        }
        ++decoded_count;
        return dsp.RecvData(0);
    }
};

int main(int argc,char**argv) {
    try {
        if (argc==2 && std::string(argv[1])=="--help") {
            std::cout<<"usage: sounda-dsp-probe FIRMWARE.a ADTS.aac [ADTS.aac ...]\n"
                     <<"Loads the supplied firmware into Teakra; writes ADTS.aac.dsp.s16le.\n"
                     <<"Tracks share the firmware and send command zero between streams.\n";
            return 0;
        }
        if(argc<3) throw std::runtime_error("usage: teakra-aac FIRMWARE ADTS...");
        auto firmware=read(argv[1]);
        Decoder decoder(firmware);
        constexpr unsigned rates[]={96000,88200,64000,48000,44100,32000,24000,22050,16000,12000,11025,8000,7350};
        for(int file=2;file<argc;++file) {
            auto adts=read(argv[file]);
            std::ofstream pcm(std::string(argv[file])+".dsp.s16le",std::ios::binary);
            decoder.send(0); // Invalidate the firmware's cached frequency.
            unsigned count=0, errors=0, rate=0, channels=0, peak=0;
            auto begin=decoder.cycles;
            for(std::size_t p=0;p<adts.size();) {
                if(adts.at(p)!=0xff || (adts.at(p+1)&0xf6)!=0xf0) throw std::runtime_error("invalid ADTS sync");
                auto header=(adts.at(p+1)&1) ? 7u : 9u;
                auto sf=(adts.at(p+2)>>2)&15;
                if(sf>=13) throw std::runtime_error("invalid ADTS sample rate");
                rate=rates[sf]; channels=((adts.at(p+2)&1)<<2)|(adts.at(p+3)>>6);
                auto length=((adts.at(p+3)&3)<<11)|(adts.at(p+4)<<3)|(adts.at(p+5)>>5);
                if(length<header || p+length>adts.size()) throw std::runtime_error("truncated ADTS");
                Bytes frame(adts.begin()+p+header,adts.begin()+p+length);
                auto reply=decoder.decode(frame,rate,channels);
                if(reply) ++errors;
                for(unsigned i=0;i<1024;++i) for(unsigned c=0;c<channels;++c) {
                    auto at=0x10000+c*0x800+2*i;
                    auto value=std::int16_t(read16(decoder.memory,at));
                    peak=std::max(peak,unsigned(value<0 ? -int(value) : value));
                    pcm.put(char(value));pcm.put(char(value>>8));
                }
                ++count;p+=length;
                if(reply) std::cout<<"{\"event\":\"decode_error\",\"frame\":"<<count<<",\"reply\":"<<reply<<"}\n";
            }
            std::cout<<"{\"event\":\"track\",\"path\":\""<<argv[file]<<"\",\"frames\":"<<count<<",\"rate\":"<<rate<<",\"channels\":"<<channels<<",\"errors\":"<<errors<<",\"peak\":"<<peak<<",\"dsp_cycles\":"<<decoder.cycles-begin<<",\"pipe_pointer\":"<<decoder.pointer<<",\"total_ahbm_reads\":"<<decoder.ahbm_reads<<",\"total_ahbm_writes\":"<<decoder.ahbm_writes<<"}\n";
            if(errors) return 2;
        }
    } catch(const std::exception& e) { std::cerr<<e.what()<<"\n";return 1; }
}
