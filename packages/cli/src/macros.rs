macro_rules! register_plugins {
    ($($id:ident($plugin:ty) => $initializer:expr),+ $(,)?) => {
        enum SsufidPluginRegistry {
            $($id($plugin),)+
        }

        impl SsufidPluginRegistry {
            async fn save_run(
                self,
                core: Arc<ssufid::SsufidCore>,
                out_dir: &Path,
                posts_limit: u32,
                retry_count: u32,
            ) -> eyre::Result<()> {
                match self {
                    $(Self::$id(plugin) => {
                        crate::save_run(core, out_dir, plugin, posts_limit, retry_count).await
                    }),+
                }
            }
        }

        fn construct_tasks(
            core: Arc<SsufidCore>,
            out_dir: &Path,
            options: SsufidDaemonOptions,
        ) -> Vec<impl std::future::Future<Output = eyre::Result<()>>> {
            let include: Option<HashSet<String>> = options
                .include
                .is_empty()
                .not()
                .then_some(HashSet::from_iter(options.include));
            let exclude: Option<HashSet<String>> = options
                .exclude
                .is_empty()
                .not()
                .then_some(HashSet::from_iter(options.exclude));
            let tasks = [
                $((
                    <$plugin>::IDENTIFIER,
                    SsufidPluginRegistry::$id($initializer),
                ),)+
            ];

            if let Some(include) = include {
                tasks
                    .into_iter()
                    .filter_map(|(id, task)| {
                        include.contains(id).then_some(task.save_run(
                            core.clone(),
                            out_dir,
                            options.posts_limit,
                            options.retry_count,
                        ))
                    })
                    .collect()
            } else if let Some(exclude) = exclude {
                tasks
                    .into_iter()
                    .filter_map(|(id, task)| {
                        exclude.contains(id).not().then_some(task.save_run(
                            core.clone(),
                            out_dir,
                            options.posts_limit,
                            options.retry_count,
                        ))
                    })
                    .collect()
            } else {
                tasks
                    .into_iter()
                    .map(|(_, task)| {
                        task.save_run(
                            core.clone(),
                            out_dir,
                            options.posts_limit,
                            options.retry_count,
                        )
                    })
                    .collect()
            }
        }
    };
}

pub(crate) use register_plugins;
