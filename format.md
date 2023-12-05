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

## Index blocks

An index block consists of the following, in order.
- An index block header.
- A sequence of index entries.
- A filter map (if any of the blocks have filters).
- An entry map (unless the index entries are fixed length).

An index block header specifies:
- The magic, size, and checksum that begins every block (64 bits).
- The number of index entries in the block (16 bits).
- The offset within the block to the filter map (32 bits).
- The offset within the block to the entry map (32 bits).

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

## Filter map

If any of the child nodes have filters, then this is an array of the
filters that apply to them, in the format:

- Number of child nodes.
- Offset and size of filter block.

The filter map contains counts because a single filter block can apply
to many child nodes.

## Entry map

This is an array of the offsets within the block to the start of each
index entry.

The entry map is omitted if the index entries are fixed-size, as in
the row indexes.

# Filters

Filters are useful in databases because a filter is much smaller than
the data that it covers.  Filters are extra useful for databases
(unlike our use case) that keep data in lots of different places to
eliminate most of those places for looking for a particular value.

With DBSP, filters are likely to be useful for operators that keep and
update the integral of an input stream, such as:

- Join: incremental join looks up the key in each update on stream A
  in the integral of stream B, and vice versa.

- Aggregation: each update to the aggregation's input stream is
  looked up in an integral of the input stream.

- Distinct: similar to aggregation.

- Upsert: similar to aggregation.

Filters only help if the value being sought is not in the file.
Otherwise, the time and space spent on the filter makes operations
strictly slower.

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

## Locating filters

We need to find and then load the filter block.  The cost of loading a
filter block is a single read.  That leaves finding the filter block,
for which we need to make a design choice.  Some possibilities are:

* One large filter per column or per file.  This would work for
  readers.  It does not work for writers because the `O(n)` size
  filter would have to be held in memory or updated repeatedly on
  disk.  The same is true of schemes that would partition the filter
  based on the data hash.

* Index-level filter.  That is, whenever we write out an index block,
  we consider whether to also write a filter block that covers that
  index block and all of the index and data blocks under it.  This
  strategy might work.  It is inflexible, since if we decide that `k`
  values are insufficient to write a filter block, then our next
  choice is going to be for no less than `32 * k` values (where 32 is
  the minimum branching factor).

  The table below shows that there is an appropriate level in the
  index hierarchy for a filter at all of the fixed-size values that we
  care about (except possibly for 16-byte values, which may not be
  worth filtering).

  ```
          # of   Values        Entries            # of values covered by a single index block
   Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
    Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
  ------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
     16     68 B     512    data    256       4  131 k   33 M    8 B    2 T                       4.0 GB
     32     34 B     256    data    128       4   32 k    4 M  536 M   68 B                       8.1 GB
     64     17 B     128    data     64       5    8 k  524 k   33 M    2 B  137 B                 16 GB
    128      8 B      64    data     32       6    2 k   65 k    2 M   67 M    2 B   68 B          33 GB
    256      4 B      32    data     32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
    512      2 B      32    data     32       6    1 k   32 k    1 M   33 M    1 B   34 B          66 GB
   1 kB      1 B      32    data     32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
   2 kB    536 M      32    data     32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
   4 kB    268 M      32    data     32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
   8 kB    134 M      32    data     32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
  16 kB     67 M      32    data     32       5    1 k   32 k    1 M   33 M    1 B                 66 GB
  32 kB     33 M      32    data     32       4    1 k   32 k    1 M   33 M                        66 GB
  64 kB     16 M      32    data     32       4    1 k   32 k    1 M   33 M                        66 GB
  ```

  But it might be difficult to tune a heuristic to decide on-line when
  to emit a filter block.  The obvious heuristic is to emit a filter
  block at some fixed value count threshold.  Suppose we set an 8k
  value threshold; then consider the 64-byte case.  8k values in a
  filter is fine (we end up with a 16 kB filter block), but 524k is
  too many (a 1 MB filter block seems excessive) if we're just under
  the threshold.  It's easier if we only consider only values that
  force our minimum branching level, but that would limit filtering to
  values that are 128 bytes or larger?

* Separate filter index.  We could have a separate index for filters.
  If we put 32k values in each filter block and then index those based
  on the values that they cover, then we get a relatively small index
  (compared to the data index size, shown in the table above) at about
  64 MB regardless of data size:

  ```
  Index coverage for 1 TB data, min_branch=32, min_data_block=8192, min_index_block=8192:

           # of   Values        Entries            # of values covered by a single index block
   Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
    Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
  ------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
     16     68 B     512  filter    256       3    8 M    2 B  549 B                               64 MB
     32     34 B     256  filter    128       3    4 M  536 M   68 B                               64 MB
     64     17 B     128  filter     64       4    2 M  134 M    8 B  549 B                        65 MB
    128      8 B      64  filter     32       4    1 M   33 M    1 B   34 B                        66 MB
    256      4 B      32  filter     32       4    1 M   33 M    1 B   34 B                        66 MB
    512      2 B      32  filter     32       4    1 M   33 M    1 B   34 B                        66 MB
   1 kB      1 B      32  filter     32       3    1 M   33 M    1 B                               66 MB
   2 kB    536 M      32  filter     32       3    1 M   33 M    1 B                               66 MB
   4 kB    268 M      32  filter     32       3    1 M   33 M    1 B                               66 MB
   8 kB    134 M      32  filter     32       3    1 M   33 M    1 B                               66 MB
  16 kB     67 M      32  filter     32       3    1 M   33 M    1 B                               67 MB
  32 kB     33 M      32  filter     32       2    1 M   33 M                                      66 MB
  64 kB     16 M      32  filter     32       2    1 M   33 M                                      68 MB
  ```

  However, this filter index would partially duplicate the data index,
  and as a percentage of the size of the filter data itself it wastes
  a lot of disk (and memory): for 64-kB values, the total filter data
  is no more than 32 MB (at 16 bits per value) and the index is over
  twice as big!

* Index-granularity filter.  This is much like the index-level filter
  except that filter blocks can cover a partial L2- or higher-level
  index block.  Each time the writer emits an L1 index block, if
  enough values have been emitted since the last filter block, it
  emits a new filter block.  References to the index blocks that
  constitute a filter block then also point to the filter block.  This
  reduces the variability of the number of values held in a filter
  block to the maximum number of values in a data block, which is 512
  (with 8-kB minimum block size and a minimum branching factor of 32).

  It's hard to quantify the exact overhead of the index-granularity
  filter.  The implemented model does not account for this or other
  kinds of overhead in data or index blocks.  There will be some
  overhead in data index nodes to point to the filter block, about 8
  bytes per filter block.  That overhead seems unlikely to increase
  the index height (for small values) or the size of index blocks (for
  large values).

Index-granularity filters seem to offer the best tradeoffs.  See
[Filter map](#filter-map) for the tentative design.
