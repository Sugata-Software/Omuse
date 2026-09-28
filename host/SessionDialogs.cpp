#include "SessionWindow.h"
#include <memory>
#include <QtMath>
#include <vector>
#include <QImage>
#include "EditorDialogs.h"
#include <QMessageBox>

void SessionWindow::showSizeDialog(bool imageSize) {
    const QJsonObject state = sessionState();
    if (state.value("busy").toBool() || state.value("width").toInt() <= 0) return;
    SizeDialog dialog(state, imageSize, this, m_platform.colors);
    // Failed allocation/validation leaves the dialog open and the document intact.
    while (dialog.exec() == QDialog::Accepted) {
        if (sendCommand(dialog.command())) { refreshImage(); return; }
        m_platform.notifier->warn(tr("Resize failed"), sessionState().value("error").toString());
    }
}

void SessionWindow::showFilterDialog(const QString &kind) {
    if (!sendCommand({{"action", "filterBegin"}, {"kind", kind}})) return;
    FilterDialog dialog(kind, [this](const QJsonObject &command) {
        const bool ok = sendCommand(command);
        refreshImage();
        return ok;
    }, this);
    dialog.exec();
}

void SessionWindow::showAdjustDialog(const QString &kind) {
    // New sheets: addAdjustment creates the adjustment layer and begins its edit
    // (macOS Layers > New Adjustment Sheet). The sheet becomes the active layer.
    // Histogram of the pixels the adjustment starts from (captured before the sheet exists),
    // alpha-weighted: bins for RGB (luma), R, G, B.
    auto bins = std::make_shared<std::vector<std::vector<double>>>(4, std::vector<double>(256, 0.0));
    if (!m_image.isNull()) {
        const QImage source = m_image.convertToFormat(QImage::Format_RGBA8888);
        for (int y = 0; y < source.height(); ++y) {
            const uchar *row = source.constScanLine(y);
            for (int x = 0; x < source.width(); ++x) {
                const uchar *px = row + x * 4;
                const double weight = px[3] / 255.0;
                if (weight <= 0) continue;
                (*bins)[0][qBound(0, qRound(0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]), 255)] += weight;
                for (int c = 0; c < 3; ++c) (*bins)[c + 1][px[c]] += weight;
            }
        }
    }
    if (!sendCommand({{"action", "addAdjustment"}, {"kind", kind}})) {
        sendCommand({{"action", "adjustmentCancel"}});
        return;
    }
    auto original = std::make_shared<QImage>(m_image);
    AdjustServices services;
    services.histogram = [bins](int channel) { return (*bins)[qBound(0, channel, 3)]; };
    services.colors = m_platform.colors;
    services.requestSample = [this, original](std::function<void(const QColor &)> done) {
        if (m_canvasWidget) m_canvasWidget->setCursor(Qt::CrossCursor);
        m_pixelSampler = [this, original, done](const QPointF &point) {
            QColor color;
            const QPoint pixel(qFloor(point.x()), qFloor(point.y()));
            if (original->rect().contains(pixel)) {
                const QColor c = original->pixelColor(pixel);
                if (c.alpha() > 0) color = QColor::fromRgbF(c.redF(), c.greenF(), c.blueF());
            }
            done(color);
        };
    };
    AdjustDialog dialog(kind, [this](const QJsonObject &command) {
        const bool ok = sendCommand(command);
        refreshImage();
        return ok;
    }, this, services);
    const int outcome = dialog.exec();
    m_pixelSampler = nullptr;
    if (m_canvasWidget) m_canvasWidget->unsetCursor();
    if (outcome == QDialog::Rejected) {
        // Discard the newly created adjustment layer on cancel (R26)
        sendCommand({{"action", "undo"}});
        refreshImage();
        refreshLayers();
    }
}

#include <QLineEdit>
#include <QListWidget>
#include <QListWidgetItem>
#include <QVBoxLayout>
#include <QKeyEvent>
#include <QAction>
#include <QMenuBar>
#include <QMenu>

CommandPaletteDialog::CommandPaletteDialog(SessionWindow *window, QWidget *parent)
    : QDialog(parent), m_window(window) {
    setWindowTitle(tr("Command Palette"));
    setObjectName("commandPaletteDialog");
    setMinimumSize(480, 320);
    resize(560, 380);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(6);

    m_filter = new QLineEdit(this);
    m_filter->setObjectName("commandPalette.filter");
    m_filter->setPlaceholderText(tr("Type a command to run..."));
    m_filter->setClearButtonEnabled(true);
    m_filter->installEventFilter(this);
    layout->addWidget(m_filter);

    m_list = new QListWidget(this);
    m_list->setObjectName("commandPalette.list");
    m_list->setUniformItemSizes(true);
    layout->addWidget(m_list);

    collectCommands();
    populateList(QString());

    connect(m_filter, &QLineEdit::textChanged, this, [this](const QString &text) {
        populateList(text);
    });
    connect(m_filter, &QLineEdit::returnPressed, this, [this] {
        triggerSelected();
    });
    connect(m_list, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *) {
        triggerSelected();
    });
}

bool CommandPaletteDialog::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_filter && event->type() == QEvent::KeyPress) {
        auto *keyEvent = static_cast<QKeyEvent *>(event);
        if (keyEvent->key() == Qt::Key_Down) {
            int row = m_list->currentRow();
            if (row < m_list->count() - 1) m_list->setCurrentRow(row + 1);
            return true;
        } else if (keyEvent->key() == Qt::Key_Up) {
            int row = m_list->currentRow();
            if (row > 0) m_list->setCurrentRow(row - 1);
            return true;
        }
    }
    return QDialog::eventFilter(watched, event);
}

void CommandPaletteDialog::collectCommands() {
    m_entries.clear();
    if (!m_window) return;

    auto traverseMenu = [this](auto self, QMenu *menu, const QString &prefix) -> void {
        if (!menu) return;
        const QString cat = prefix.isEmpty() ? menu->title().remove('&') : prefix + " > " + menu->title().remove('&');
        for (QAction *act : menu->actions()) {
            if (act->isSeparator()) continue;
            if (act->menu()) {
                self(self, act->menu(), cat);
            } else if (!act->text().isEmpty()) {
                QString cleanText = act->text().remove('&');
                QString shortcut = act->shortcut().toString(QKeySequence::NativeText);
                m_entries.push_back({cleanText, cat, shortcut, act});
            }
        }
    };

    if (m_window->menuBar()) {
        for (QAction *menuAct : m_window->menuBar()->actions()) {
            if (menuAct->menu()) {
                traverseMenu(traverseMenu, menuAct->menu(), QString());
            }
        }
    }

    for (QAction *act : m_window->findChildren<QAction *>()) {
        if (act->isSeparator() || act->text().isEmpty()) continue;
        bool already = false;
        for (const auto &e : m_entries) {
            if (e.action == act) { already = true; break; }
        }
        if (!already) {
            QString cleanText = act->text().remove('&');
            QString shortcut = act->shortcut().toString(QKeySequence::NativeText);
            m_entries.push_back({cleanText, tr("Action"), shortcut, act});
        }
    }
}

void CommandPaletteDialog::populateList(const QString &query) {
    m_list->clear();
    const QString lower = query.toLower().trimmed();
    for (size_t i = 0; i < m_entries.size(); ++i) {
        const auto &cmd = m_entries[i];
        const QString full = (cmd.category + " " + cmd.title).toLower();
        if (!lower.isEmpty() && !full.contains(lower)) continue;

        QString label = cmd.category.isEmpty() ? cmd.title : QString("[%1] %2").arg(cmd.category, cmd.title);
        if (!cmd.shortcut.isEmpty()) {
            label += QString(" (%1)").arg(cmd.shortcut);
        }

        auto *item = new QListWidgetItem(label, m_list);
        item->setData(Qt::UserRole, static_cast<int>(i));
        if (!cmd.action->isEnabled()) {
            item->setForeground(Qt::gray);
        }
    }
    if (m_list->count() > 0) {
        m_list->setCurrentRow(0);
    }
}

void CommandPaletteDialog::triggerSelected() {
    QListWidgetItem *cur = m_list->currentItem();
    if (!cur) return;
    int idx = cur->data(Qt::UserRole).toInt();
    if (idx >= 0 && idx < static_cast<int>(m_entries.size())) {
        QAction *act = m_entries[idx].action;
        accept();
        if (act && act->isEnabled()) {
            act->trigger();
        }
    }
}

void SessionWindow::showCommandPalette() {
    CommandPaletteDialog dialog(this, this);
    dialog.exec();
}

