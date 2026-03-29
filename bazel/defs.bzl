def c_example(name, src):
    native.genrule(
        name = name,
        srcs = [
            src,
            "common.h",
            "mcapable_ffi.h",
            "//crates/mcapable-ffi:ffi",
        ],
        outs = [name],
        cmd = "LIB=$(location //crates/mcapable-ffi:ffi) && " +
              "LIBDIR=$$(dirname $$LIB) && " +
              "cc $(location {src}) " +
              "-I$(dirname $(location :mcapable_ffi.h)) " +
              "$$LIB " +
              "-Wl,-rpath,$$LIBDIR " +
              "-o $@".format(src = src),
        local = True,
        tags = ["local"],
    )
