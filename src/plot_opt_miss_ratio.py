import sys
import math
import pandas as pd
import matplotlib.pyplot as plt

def main(data_csv, output_plot):
    df = pd.read_csv(data_csv)

    # Convert cache_size to log2 scale for x-axis
    # df['cache_size'] = df['cache_size'] * 16
    df['log_cache_size'] = df['cache_size'].apply(lambda x: math.log2(x))
    # df['log_cache_size'] = df['cache_size']

    # Multiply miss_ratio by 100 to convert to percentage
    df['miss_ratio'] = df['miss_ratio'] * 100

    # Plotting
    plt.figure(figsize=(12, 8))
    plt.plot(df['log_cache_size'], df['miss_ratio'], color='blue', label='CLAM Miss Ratio Curve', linewidth=2)

    # Configure x-axis to show cache sizes that are 1 or powers of 2
    def is_power_of_two(n):
        return n != 0 and (n & (n - 1)) == 0

    mask = df['cache_size'].apply(is_power_of_two)
    x_ticks = df.loc[mask, 'log_cache_size']
    x_labels = df.loc[mask, 'cache_size']
    plt.xticks(ticks=x_ticks, labels=x_labels.astype(int), fontsize=16)

    plt.xlabel('Cache Blocks Number ', fontsize=20) ## of Blocks (64 Bytes)
    plt.ylabel('Miss Ratio (%)', fontsize=20)
    # plt.title('OPT Miss Ratio Curve', fontsize=24)
    plt.legend(fontsize=18)
    plt.grid(True, linewidth=1.5)
    plt.ylim(-3, 103)
    plt.tight_layout()

    for _, row in df[mask].iterrows():
        plt.annotate(f"{row['miss_ratio']:.4f}%",
                     (row['log_cache_size'], row['miss_ratio']),
                     textcoords="offset points",
                     xytext=(0,10),
                     ha='center',
                     fontsize=14
                     )

    plt.tight_layout()

    # Save the plot
    plt.savefig(output_plot, dpi=300)
    print(f"Plot saved to {output_plot}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python plot_opt_miss_ratio.py <data_csv> <output_plot>")
        sys.exit(1)

    data_csv = sys.argv[1]
    output_plot = sys.argv[2]
    main(data_csv, output_plot)