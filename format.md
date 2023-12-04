# Layer file format

An `n`-layer file stores `n` columns of data:

- Column 1 is a sorted, indexed set of values.  For uniformity with
  the other columns, we call its entire contents a "row group".

- If `n > 1`, then each value in column 1, row `j`, is associated with
  the `j`th group of one or more rows in column 2.  Each "row group"
  is sorted and indexed by value.

- And so on: for `i` in `2..n`, each value in column `i`, row `j`, is
  associated with the `j`th row group in column `i + 1`.

This arrangement corresponds to DBSP data structures like this, if
`Ti` is the type of the data stored in column `i`:

- `ColumnLayer<K, R>` is a 1-layer file with with `T1 = (K, R)`.

- `OrderedLayer<K, ColumnLayer<V, R>, O>` is a 2-layer file with `T1 =
  K` and `T2 = (V, R)`.

- `OrderedLayer<K, OrderedLayer<V, ColumnLayer<T, R>, O>, O>` is one
  of:

  * A 2-layer file with `T1 = K` and `T2 = (V, [(T, R)])`.

  * A 3-layer file with `T1 = K`, `T2 = V`, and `T3 = (T, R)`.

It is likely that only 1-, 2, and 3-layer files matter, and maybe not
even 3-layer.

# Goals

- Variable-length values in all columns.
- Writing a file:
  * Only needs one pass over the input data.
  * Does not write parts of the output file more than once.
  * Does not rely on sparse file semantics in the file system.
  * Uses `O(1)` memory.
- Read and write performance should be balanced; that is, neither
  should be significantly sacrified to benefit the other.
- Efficient indexing within row groups:
    * `O(lg n)` seek by data value[^0]
    * Linear reads.
- Approximate set membership query in `~O(1)` time[^0].
- Data checksums.
- Support 1 TB total size.

[^0]: It should be possible to turn this feature off for files that
    don't need it.

Possible goals for later:

- Data compression.

# Branching factor and block size

Our file format is tree shaped, with data blocks as leaf nodes and
index blocks as interior nodes in the tree.  The tree's branching
factor is the number of values per data block and the number of index
entries per index block.

Increasing the branching factor reduces the number of nodes that must
be read to find a particular value.  It also increases the memory
required to read each block.

The branching factor is not an important consideration for small data
values, because our 4-kB minimum block size means that the minimum
branching factor will be high.  For example, over 100 32-byte values
fit in a 4-kB block, even considering overhead.

The branching factor is more important for large values.  Suppose
only a single value fits into a data block.  The index block that
refers to it reproduces the first and last value in each data
block, which in turn makes it likely that index block only fits a
single child, which is pathological and silly.

Thus, we need to ensure at least a minimum branching factor even with
large values.  The program in this repository will estimate some
numbers for us.  For 1 TB of data, with a 16-byte minimum branching
factor and 4-kB minimum block sizes, the maximum height ranges from 4
to 7, depending on the size of each data value:

```
Details of index coverage for min_branch=16, min_data_block=4096, min_index_block=4096:
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
   16     68 B     256  data      128       4   32 k    4 M  536 M   68 B                       8.1 GB
   32     34 B     128  data       64       5    8 k  524 k   33 M    2 B  137 B                 16 GB
   64     17 B      64  data       32       6    2 k   65 k    2 M   67 M    2 B   68 B          33 GB
  128      8 B      32  data       16       7    512    8 k  131 k    2 M   33 M  536 M    8 B   68 GB
  256      4 B      16  data       16       7    256    4 k   65 k    1 M   16 M  268 M    4 B  136 GB
  512      2 B      16  data       16       7    256    4 k   65 k    1 M   16 M  268 M    4 B  136 GB
 1 kB      1 B      16  data       16       7    256    4 k   65 k    1 M   16 M  268 M    4 B  136 GB
 2 kB    536 M      16  data       16       7    256    4 k   65 k    1 M   16 M  268 M    4 B  136 GB
 4 kB    268 M      16  data       16       6    256    4 k   65 k    1 M   16 M  268 M         136 GB
 8 kB    134 M      16  data       16       6    256    4 k   65 k    1 M   16 M  268 M         136 GB
16 kB     67 M      16  data       16       6    256    4 k   65 k    1 M   16 M  268 M         136 GB
32 kB     33 M      16  data       16       6    256    4 k   65 k    1 M   16 M  268 M         136 GB
64 kB     16 M      16  data       16       5    256    4 k   65 k    1 M   16 M                136 GB
```

If we increase the minimum index block size to 8 kB, it reduces the
index height for the 32-, 64- and 128-byte value sizes:

```
Details of index coverage for min_branch=16, min_data_block=4096, min_index_block=8192:
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
   16     68 B     256  data      256       4   65 k   16 M    4 B    1 T                       8.0 GB
   32     34 B     128  data      128       4   16 k    2 M  268 M   34 B                        16 GB
   64     17 B      64  data       64       5    4 k  262 k   16 M    1 B   68 B                 32 GB
  128      8 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
...
```

Increasing the minimum block size for both data and index blocks to 8
kB doesn't further reduce the index height.

On the other hand, if we increase the minimum branching factor to 32
and keep the minimum block sizes at 4 kB, it reduces the maximum
height to 6 or below for every value size:

```
Details of index coverage for min_branch=32, min_data_block=4096, min_index_block=4096:
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
   16     68 B     256  data      128       4   32 k    4 M  536 M   68 B                       8.1 GB
   32     34 B     128  data       64       5    8 k  524 k   33 M    2 B  137 B                 16 GB
   64     17 B      64  data       32       6    2 k   65 k    2 M   67 M    2 B   68 B          33 GB
  128      8 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
  256      4 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
  512      2 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
 1 kB      1 B      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
 2 kB    536 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
 4 kB    268 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
 8 kB    134 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
16 kB     67 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
32 kB     33 M      32  data       32       4    1 k   32 k    1 M   33 M                        66 GB
64 kB     16 M      32  data       32       4    1 k   32 k    1 M   33 M                        66 GB
```

If we also increase the minimum block sizes to 8 kB, it further
reduces the index height for 32- and 64-byte values:


```
Details of index coverage for min_branch=32, min_data_block=8192, min_index_block=8192:
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
   16     68 B     512  data      256       4  131 k   33 M    8 B    2 T                       4.0 GB
   32     34 B     256  data      128       4   32 k    4 M  536 M   68 B                       8.1 GB
   64     17 B     128  data       64       5    8 k  524 k   33 M    2 B  137 B                 16 GB
...
```

Thus, a good starting point seems to be:

- Minimum branching factor of 32.

- 8 kB minimum block size for data and index blocks.

With those parameters established, we continue to describe the file format.

# Overall file format

The file is a sequence of binary blocks, each a power-of-2 multiple of
4 kB in size, in the following order:

- File header block
- Interleaved data blocks, index blocks, and filter blocks.
- File trailer block

Blocks need not be the same size.  Each block begins with:

- A magic number that identifies its type.
- Size.
- Checksum.

The file header block contains:

- Number of columns.
- Version number.
- Key-value pairs?
  * Miscellaneous configuration.
  * Identifying name for debugging purposes

The file trailer block contains:

- The number of columns in the file.
- For each column:
  * The offset and size of its highest-level value index block (if any).
  * The offset and size of its highest-level row index block.
  * The offset and size of the highest-level filter index block (if any).
  * The total number of rows in the column.

# Data blocks

A data block consists of a data block header, a sequence of values,
and a data block trailer.  The info in the trailer would more
naturally be appended to the header, but we don't know in advance how
many values will fit in a block.

A data block header specifies the number of values in the block,
plus the magic, size, and checksum that begins every block.

## Values

We use a separate call to `rkyv` to independently serialize each
value.  We align each value whose archived type is `T` on a
`mem::align_of::<T>()` byte boundary, which is also the alignment that
`rkyv` expects and requires.

> `rkyv` can serialize arrays and slices perfectly well on its own.
> We serialize each value in the array separately because this allows
> us better control over the size of a data block.  Un-archiving is
> just a pointer subtraction and a cast, so there's no performance
> benefit from unarchiving a full slice at a time.

## Data block trailer

The trailer specifies the following per value:

- Offset within the block of the `rkyv` root value.

  > This does not necessarily point to the beginning of the value, and
  > it does not point to the very end.  It can't be used to find all
  > of the bytes that constitute the value, at least not directly.  If
  > we need that for some reason then we'll need to use offset/length
  > pairs instead of just offsets.

- `start..end` range pointing to the rows associated with this vluae
  in the next column, if this is not the last column.

# Indexes

We need to access different columns a few different ways:
  
1. We need to be able to search each row group by data value in
   forward and reverse order.
  
2. We need to be able to read each row group sequentially in forward
   and reverse order.  This requires an index because the data blocks
   that comprise a column are not necessarily consecutive in the layer
   file.
  
3. We need to be able to follow a pointer from column `i` to its
   associated row group in column `i + 1`.
   
A single index on values and row numbers can support all of these
purposes.  However, if data values are large, the values make this
index very large, up to 6.6% overhead[^1].  An index on just row
numbers is much smaller, never more than .2% overhead[^2].

[^1]: 66 GB, for 1 TB of values between 256 bytes and 64 kB in size.
[^2]: Between 8 MB and 2 GB for 1 TB of values between 16 bytes and 64
    kB in size.

Thus, we construct two indexes on each column:

* A **value index** by value and row number, to serve purpose (1).

* A **row index** by row number, to serve purposes (2) and (3).

Some operators will only use the second index.  We don't need to
construct it if the operator says so as a hint.

In addition, if the column has a filter, we add an index of blocks in
the filter (see [Filters](#filters)).

## Index blocks

An index block consists of a index block header, a sequence of index
entries, and a index block trailer.

An index block header specifies the number of index entries in the block,
plus the magic, size, and checksum that begins every block.

## Index entries

An index entry consists of:

- The child block's offset, size, and whether the child is an index or
  data block.  These can be packed into no more than 6 bytes (40 bits
  for the offset, which gives a 52-bit or 4-PB maximum file size; no
  more than 6 bits for a shift count for the size; 1 bit for
  index/data).

- Except in column 1, the row number of the first row in the child
  block.  This also seems safe to pack into 6 bytes, allowing for
  about 280 trillion rows.

  Column 1 does not need row numbers because it is never looked up
  that way.

- In the data index only, the first and last value in the child.

It might seem silly to optimize the sizes of the offsets and row
numbers, but it reduces the size of the row index by about 25% in the
first column and about 50% in the other columns, up to 256 MB and 1
GB, respectively, in some cases.

## Index block trailer

In the data index only, the index block trailer specifies the offset
of each index entry within the block.  (Index entries are fixed-size
in the row indices, so their offsets can be calculated.)

# Filters

Filters are useful in databases because a filter is much smaller than
the data that it covers.  Filters are extra useful for databases
(unlike ours) that keep data in lots of different places to eliminate
most of those places for looking for a particular value.

## Filter space cost

The space cost of a filter is between 8 and 16 bits per value stored,
regardless of the value's size.

The false positive rate depends on the space per value.  With an RSQF:

  - 16 bits per value yields a false-positive rate of .02%.
  
  - 8 bits per value yields a false-positive rate of 1.5%.

The proportional cost depends on the size of the data:

  - For 16-byte values: 6% to 12% of the data size.

  - For 32-byte values: 3% to 6% of the data size.

  - For 128-byte values: .8% to 1.5% of the data size.
  
  - For 1-kB values: .1% to .2% of the data size.
  
  - For larger values, less than .1% of the data size.
  
We could choose to omit small values from the filter.
  
## Filter time cost

The time cost of a filter is the time to find and then load the filter
block.

We can use fixed-size filter blocks, say ones that store 65536 values
each, which at 8 to 16 bits each is 64 kB to 128 kB per block.

We need an index to be able to find the filter blocks.  With 65536
values per block and 40 bits per index entry, we can keep the index
small, no more than 5 MB total for 1 TB of data even with 16-byte
values (that might be too small to use filtering), and no more than
two levels (the top level of which is a a single block).  This means
that finding the filter block will usually be cheap.

Loading the filter block is a single read.

The following compares the cost of data versus filter lookups.  We see
the biggest benefit at mid-size values (from 128 to 512 bytes), where
a filter index lookup avoids traversing 4 levels of indexes.  At other
data sizes, the filter still avoids 2 or 3 traversals.

Filtering will only help if the value being sought is not in the file.
Otherwise, the time and space spent on the filter makes operations
strictly slower.

```
Details of index coverage for min_branch=32, min_data_block=8192, min_index_block=8192:
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
   16     68 B     512  data      256       4  131 k   33 M    8 B    2 T                       4.0 GB
                        filter   1638       2  107 M  175 B                                     5.0 MB
   32     34 B     256  data      128       4   32 k    4 M  536 M   68 B                       8.1 GB
                        filter   1638       2  107 M  175 B                                     2.5 MB
   64     17 B     128  data       64       5    8 k  524 k   33 M    2 B  137 B                 16 GB
                        filter   1638       2  107 M  175 B                                     1.3 MB
  128      8 B      64  data       32       6    2 k   65 k    2 M   67 M    2 B   68 B          33 GB
                        filter   1638       2  107 M  175 B                                     655 kB
  256      4 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
                        filter   1638       2  107 M  175 B                                     335 kB
  512      2 B      32  data       32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
                        filter   1638       2  107 M  175 B                                     175 kB
 1 kB      1 B      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
                        filter   1638       2  107 M  175 B                                      95 kB
 2 kB    536 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
                        filter   1638       2  107 M  175 B                                      55 kB
 4 kB    268 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
                        filter   1638       2  107 M  175 B                                      31 kB
 8 kB    134 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
                        filter   1638       2  107 M  175 B                                      23 kB
16 kB     67 M      32  data       32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
                        filter   1638       1  107 M                                              7 kB
32 kB     33 M      32  data       32       4    1 k   32 k    1 M   33 M                        66 GB
                        filter   1638       1  107 M                                              7 kB
64 kB     16 M      32  data       32       4    1 k   32 k    1 M   33 M                        66 GB
                        filter   1638       1  107 M                                              7 kB
```

