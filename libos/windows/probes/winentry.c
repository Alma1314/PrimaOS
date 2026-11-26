__declspec(dllimport) unsigned long Stub(unsigned long value);
__declspec(dllimport) unsigned long Extra(unsigned long value);
__declspec(dllimport) unsigned long Fwd(unsigned long value);

static __declspec(noinline) unsigned long helper(unsigned long value) {
    return value + 1;
}

unsigned long (*volatile local_pointer)(unsigned long) = helper;

__declspec(noinline) unsigned long entry(void) {
    unsigned long bias = local_pointer(0) - 1;
    return Stub(31) + Extra(5) + Fwd(2) + bias;
}
