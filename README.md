# orbits

`orbits` is a general purpose layeredfs crate that supports three layers: physical, patch, and virtual.

## Physical layer
Alternatively, the physical layer could also be referred to as the `archive` layer, as its intent is to be used as the last resort when loading a file, the `Default` of the file loader, if you will.

## Patch layer
The patch layer is intended to be where you can scan roots on disc. Using `orbits`'s `Tree` under the hood, it will generate a file tree which allows easy traversal via `walk_paths` and will automatically detect (and reject) conflicts depending on how it's configured.

## Virtual layer
The virtual layer is mostly for those who want to implement on-the-fly data generation or file loading callbacks.

### What's stopping my from using a virtual loader on patch or vice versa?
Well, nothing is. The whole point of `orbits` is that you can use it to generate your own layered filesystem application, and ultimately it's up to you to decide what the best way to go about doing that is.

For my uses, `orbits` having a dedicated `virtual` layer will make for extremely easy API callbacks and the like. The patch section is helpful for automatically rejecting file conflicts, and the physical layer is for the archive. But really, you can organize it whatever way you want, this is just how I suggest :)

## The `FileLoader` trait
The `FileLoader` trait allows for the implementer to design their own object/loader for any of the three sections. It only requires a few functions, and `orbits` also provides a `orbits::StandardLoader` right out of the box which uses `std::fs`.

## Conflicts
The whole point of a layered filesystem is that a bunch of different roots can all come together and function as one cohesive file tree. However, what if something is conflicting?

Fear not! For orbits offers a variety of different conflict handlers for managing conflicts.

### `Strict`
The strict conflict handler will cause `orbits` to panic on a conflict. This is currently planned to be replaced with returning an `Error` for better error handling.

### `NoRoot`
The `NoRoot` conflict handler will cause `orbits` to reject every single file from the root of a conflicting file.

### `First` and `Last`
The `First` conflict handler will cause `orbits` to keep the first file that matches the local path in the file tree, while `Last` will replace it.